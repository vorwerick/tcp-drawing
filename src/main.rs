mod entity;
mod network;

use crossbeam_channel::{Sender, unbounded};
use dashmap::DashMap;
use entity::*;
use macroquad::prelude::*;
use std::env::args;
use std::net::TcpListener;
use std::sync::Arc;

#[macroquad::main("TCP-Drawing")]
async fn main() {
    let args: Vec<String> = args().collect();
    let default_addr = "127.0.0.1:8090".to_string();
    let addr = args.get(1).cloned().unwrap_or(default_addr);

    let entities: Arc<DashMap<usize, Entity>> = Arc::new(DashMap::new());
    let mut client_press_cooldown: f32 = 0f32;
    let mut shape_size = 24f32;

    let (mut tx, rx) = unbounded::<Entity>();

    let (is_server, client_list) = match TcpListener::bind(&addr) {
        Ok(listener) => {
            println!("Running as server on {}", &addr);
            let clients = network::start_server(listener, entities.clone(), rx);
            (true, Some(clients))
        }
        Err(_) => {
            println!("Running as client, connecting to {}", &addr);
            let (client_tx, _client_rx) = unbounded::<Entity>();
            network::start_client(entities.clone(), client_tx.clone(), addr.clone());
            tx = client_tx;
            (false, None)
        }
    };

    loop {
        handle_input(
            &entities,
            &tx,
            is_server,
            &mut client_press_cooldown,
            &mut shape_size,
        );
        process(
            macroquad::time::get_frame_time(),
            &mut client_press_cooldown,
            &entities,
        );
        render(&entities, is_server, shape_size, client_list.as_ref()).await;
    }
}

fn process(delta: f32, cooldown_press: &mut f32, _entities: &DashMap<usize, Entity>) {
    // cooldown
    *cooldown_press -= delta;
    if *cooldown_press < 0.0 {
        *cooldown_press = 0.0;
    }
}

fn handle_input(
    entities: &DashMap<usize, Entity>,
    tx: &Sender<Entity>,
    is_server: bool,
    client_press_cooldown: &mut f32,
    shape_size: &mut f32,
) {
    if is_mouse_button_released(MouseButton::Left) {
        *shape_size = 32f32;
    }

    if is_mouse_button_down(MouseButton::Left) && *client_press_cooldown <= 0.0 {
        *client_press_cooldown = 0.005f32;
        let (x, y) = mouse_position();
        let id = Entity::spawn(
            x,
            y,
            shape_size.clone(),
            color_to_hex(if is_server { RED } else { GREEN }),
            entities,
        );
        *shape_size -= 0.5f32;
        if *shape_size < 4f32 {
            *shape_size = 4f32;
        }

        if let Some(id) = id {
            if let Some(entity) = entities.get(&id) {
                let entity_clone = entity.value().clone();
                if let Err(e) = tx.send(entity_clone) {
                    eprintln!("Error sending entity to network thread: {}", e);
                }
            }
        }
    }
}

fn render_entities(entities: &DashMap<usize, Entity>) {
    for entry in entities.iter() {
        let e = entry.value();
        draw_circle(e.x, e.y, e.radius, hex_to_color(e.color));
    }
}

async fn render(entities: &DashMap<usize, Entity>, is_server: bool, shape_size: f32, client_list: Option<&network::ClientList>) {
    clear_background(WHITE);

    render_entities(entities);
    let (mousex, mousey) = mouse_position();
    draw_circle_lines(mousex, mousey, shape_size, 1.0, BLACK);

    if is_server {
        draw_text("SERVER", 32f32, 32f32, 22f32, BLACK);

        // Display client IPs under the SERVER text
        if let Some(clients) = client_list {
            if let Ok(clients) = clients.lock() {
                let mut y_offset = 54f32; // Start below the SERVER text
                for client in clients.iter() {
                    let client_text = format!("Client: {}", client.addr);
                    draw_text(&client_text, 32f32, y_offset, 16f32, BLACK);
                    y_offset += 20f32; // Move down for the next client
                }
            }
        }
    } else {
        draw_text("CLIENT", 32f32, 32f32, 22f32, BLACK);
    }

    next_frame().await;
}

fn hex_to_color(hex: i32) -> Color {
    let r = ((hex >> 16) & 0xFF) as f32 / 255.0;
    let g = ((hex >> 8) & 0xFF) as f32 / 255.0;
    let b = (hex & 0xFF) as f32 / 255.0;

    Color::new(r, g, b, 1.0)
}
fn color_to_hex(color: Color) -> i32 {
    let r = (color.r * 255.0).round() as i32;
    let g = (color.g * 255.0).round() as i32;
    let b = (color.b * 255.0).round() as i32;

    (r << 16) | (g << 8) | b
}
