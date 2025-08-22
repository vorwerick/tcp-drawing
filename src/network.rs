use crate::entity::Entity;
use crossbeam_channel::{Receiver, Sender};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::net::SocketAddr;

const BUFFER_CAPACITY: usize = 16384;
const MAX_BUFFER_SIZE: usize = 100_000;
const SLEEP_DURATION: u64 = 20;

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub addr: SocketAddr,
}

pub type ClientList = Arc<Mutex<Vec<ClientInfo>>>;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Message {
    NewEntity(Entity),
    AllEntities(Vec<Entity>),
    RequestAllEntities,
}

struct MessageHandler {
    buffer: Vec<u8>,
    current_msg_len: Option<usize>,
}

impl MessageHandler {
    fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(BUFFER_CAPACITY),
            current_msg_len: None,
        }
    }

    fn extend_buffer(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    fn check_buffer_size(&mut self) -> bool {
        if self.buffer.len() > MAX_BUFFER_SIZE {
            eprintln!("Message buffer too large ({}), clearing", self.buffer.len());
            self.buffer.clear();
            self.current_msg_len = None;
            return true;
        }
        false
    }

    fn next_message(&mut self) -> Option<Result<Message, String>> {
        if self.current_msg_len.is_none() {
            if self.buffer.len() < 4 {
                return None;
            }

            let len_bytes: [u8; 4] = self.buffer[0..4].try_into().unwrap();
            let msg_len = u32::from_le_bytes(len_bytes) as usize;

            if msg_len > MAX_BUFFER_SIZE {
                eprintln!(
                    "Received suspiciously large message size: {}, resetting buffer",
                    msg_len
                );
                self.buffer.clear();
                self.current_msg_len = None;
                return None;
            }

            self.current_msg_len = Some(msg_len);
            self.buffer.drain(0..4);
        }

        if let Some(msg_len) = self.current_msg_len {
            if self.buffer.len() < msg_len {
                return None; // Not enough data yet
            }

            let message_data = self.buffer.drain(0..msg_len).collect::<Vec<u8>>();
            self.current_msg_len = None;

            match serde_json::from_slice::<Message>(&message_data) {
                Ok(message) => Some(Ok(message)),
                Err(e) => Some(Err(format!("Error decoding message: {}", e))),
            }
        } else {
            None
        }
    }
}

fn frame_message(message: &Message) -> io::Result<Vec<u8>> {
    let data =
        serde_json::to_vec(message).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let msg_len = data.len() as u32;
    let mut framed_data = Vec::with_capacity(4 + data.len());
    framed_data.extend_from_slice(&msg_len.to_le_bytes());
    framed_data.extend_from_slice(&data);

    Ok(framed_data)
}

fn send_message(stream: &mut TcpStream, message: &Message) -> io::Result<()> {
    let framed_data = frame_message(message)?;
    stream.write_all(&framed_data)?;
    stream.flush()?;
    Ok(())
}

fn send_to_clients(clients: &mut Vec<TcpStream>, message: &Message) -> usize {
    let mut successful_sends = 0;

    clients.retain_mut(|client| match send_message(client, message) {
        Ok(_) => {
            successful_sends += 1;
            true
        }
        Err(e) => {
            eprintln!("Error sending to client: {}", e);
            false
        }
    });

    successful_sends
}

fn get_all_entities(entities: &DashMap<usize, Entity>) -> Vec<Entity> {
    entities.iter().map(|e| e.value().clone()).collect()
}

fn handle_client_message(
    message: Message,
    client_idx: usize,
    clients: &mut [TcpStream],
    entities: &DashMap<usize, Entity>,
) -> io::Result<()> {
    match message {
        Message::NewEntity(entity) => {
            let id = entity.id;
            entities.insert(id, entity.clone());

            let message = Message::NewEntity(entity);
            for (j, client) in clients.iter_mut().enumerate() {
                if j != client_idx {
                    if let Err(e) = send_message(client, &message) {
                        eprintln!("Error forwarding entity to client: {}", e);
                    }
                }
            }
        }
        Message::RequestAllEntities => {
            let all_entities = get_all_entities(entities);
            let message = Message::AllEntities(all_entities);
            send_message(&mut clients[client_idx], &message)?;
        }
        Message::AllEntities(_) => {}
    }
    Ok(())
}

pub fn start_server(
    listener: TcpListener,
    entities: Arc<DashMap<usize, Entity>>,
    rx: Receiver<Entity>,
) -> ClientList {
    let client_list = Arc::new(Mutex::new(Vec::new()));
    let client_list_clone = client_list.clone();

    listener
        .set_nonblocking(true)
        .expect("Failed to set non-blocking mode");

    thread::spawn(move || {
        let mut clients = Vec::new();
        let mut client_handlers = Vec::new();
        let mut client_addresses = Vec::new();

        loop {
            match listener.accept() {
                Ok((stream, addr)) => {
                    println!("New client connected: {}", addr);
                    stream
                        .set_nonblocking(true)
                        .expect("Failed to set client to non-blocking mode");

                    let client_info = ClientInfo { addr };
                    if let Ok(mut client_list) = client_list_clone.lock() {
                        client_list.push(client_info.clone());
                    }
                    client_addresses.push(client_info);

                    if !entities.is_empty() {
                        let all_entities = get_all_entities(&entities);
                        let message = Message::AllEntities(all_entities);
                        if let Err(e) = send_message(
                            &mut stream.try_clone().expect("Failed to clone stream"),
                            &message,
                        ) {
                            eprintln!("Error sending initial entities to new client: {}", e);
                        }
                    }

                    clients.push(stream);
                    client_handlers.push(MessageHandler::new());
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    eprintln!("Error accepting connection: {}", e);
                }
            }

            while let Ok(entity) = rx.try_recv() {
                let id = entity.id;
                entities.insert(id, entity.clone());

                let message = Message::NewEntity(entity);
                send_to_clients(&mut clients, &message);
            }

            let mut to_remove = Vec::new();
            for i in 0..clients.len() {
                let mut buffer = [0; 4096];

                match clients[i].read(&mut buffer) {
                    Ok(0) => {
                        println!("Client disconnected");
                        to_remove.push(i);
                    }
                    Ok(n) => {
                        client_handlers[i].extend_buffer(&buffer[..n]);

                        while let Some(message_result) = client_handlers[i].next_message() {
                            match message_result {
                                Ok(message) => {
                                    if let Err(e) =
                                        handle_client_message(message, i, &mut clients, &entities)
                                    {
                                        eprintln!("Error handling client message: {}", e);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("{}", e);
                                }
                            }
                        }

                        client_handlers[i].check_buffer_size();
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => {
                        eprintln!("Error reading from client: {}", e);
                        to_remove.push(i);
                    }
                }
            }

            for i in to_remove.iter().rev() {
                if let Ok(mut client_list) = client_list_clone.lock() {
                    if *i < client_addresses.len() {
                        let addr = client_addresses[*i].addr;
                        client_list.retain(|client| client.addr != addr);
                    }
                }

                clients.remove(*i);
                client_handlers.remove(*i);
                if *i < client_addresses.len() {
                    client_addresses.remove(*i);
                }
            }

            //cpu tick
            thread::sleep(Duration::from_millis(SLEEP_DURATION));
        }
    });

    client_list
}

pub fn start_client(entities: Arc<DashMap<usize, Entity>>, _tx: Sender<Entity>, addr: String) {
    thread::spawn(move || match TcpStream::connect(&addr) {
        Ok(mut stream) => {
            println!("Connected to server");
            stream
                .set_nonblocking(true)
                .expect("Failed to set non-blocking mode");

            let send_stream = stream.try_clone().expect("Failed to clone stream");
            let entities_clone = entities.clone();

            let (server_tx, server_rx) = crossbeam_channel::unbounded::<Entity>();

            thread::spawn(move || {
                let mut send_stream = send_stream;
                let mut sent_entities = std::collections::HashSet::new();

                loop {
                    for entry in entities_clone.iter() {
                        let entity = entry.value().clone();
                        let id = entity.id;

                        if !sent_entities.contains(&id) {
                            let message = Message::NewEntity(entity.clone());
                            if let Err(e) = send_message(&mut send_stream, &message) {
                                eprintln!("Error sending entity to server: {}", e);
                                break;
                            }

                            sent_entities.insert(id);

                            if let Err(e) = server_tx.send(entity) {
                                eprintln!("Error forwarding entity: {}", e);
                            }
                        }
                    }

                    sent_entities.retain(|id| entities_clone.contains_key(id));

                    thread::sleep(Duration::from_millis(10));
                }
            });

            let mut request_initial = true;
            let mut message_handler = MessageHandler::new();
            let mut buffer = [0; 4096];

            loop {
                while let Ok(entity) = server_rx.try_recv() {
                    entities.insert(entity.id, entity);
                }

                match stream.read(&mut buffer) {
                    Ok(0) => {
                        println!("Server disconnected");
                        break;
                    }
                    Ok(n) => {
                        request_initial = false;
                        message_handler.extend_buffer(&buffer[..n]);

                        while let Some(message_result) = message_handler.next_message() {
                            match message_result {
                                Ok(message) => match message {
                                    Message::NewEntity(entity) => {
                                        entities.insert(entity.id, entity);
                                    }
                                    Message::AllEntities(all_entities) => {
                                        entities.clear();
                                        for entity in all_entities {
                                            entities.insert(entity.id, entity);
                                        }
                                    }
                                    Message::RequestAllEntities => {
                                        let all_entities = get_all_entities(&entities);
                                        let message = Message::AllEntities(all_entities);
                                        if let Err(e) = send_message(&mut stream, &message) {
                                            eprintln!("Error sending all entities: {}", e);
                                        }
                                    }
                                },
                                Err(e) => {
                                    eprintln!("{}", e);
                                }
                            }
                        }

                        message_handler.check_buffer_size();
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        if request_initial {
                            let message = Message::RequestAllEntities;
                            if let Err(e) = send_message(&mut stream, &message) {
                                eprintln!("Error requesting initial entities: {}", e);
                            } else {
                                request_initial = false;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error reading from server: {}", e);
                        break;
                    }
                }

                thread::sleep(Duration::from_millis(SLEEP_DURATION));
            }
        }
        Err(e) => {
            eprintln!("Failed to connect to server: {}", e);
        }
    });
}
