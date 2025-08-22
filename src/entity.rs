use dashmap::DashMap;
use macroquad::math::Vec2;
use macroquad::prelude::Circle;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Entity {
    pub(crate) id: usize,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) radius: f32,
    pub(crate) color: i32,
}

impl Create for Entity {
    fn spawn(
        x: f32,
        y: f32,
        radius: f32,
        color: i32,
        entities: &DashMap<usize, Entity>,
    ) -> Option<usize> {
        let next_id = entities.len();
        let new = Entity {
            id: next_id,
            x,
            y,
            radius,
            color,
        };
        entities.insert(next_id, new);
        Option::from(next_id)
    }
}

impl Eraser for Entity {
    fn erase(
        &mut self,
        area: Circle,
        entities: &DashMap<usize, Entity>,
    ) -> Option<(usize, Entity)> {
        let pos = Vec2::from((self.x, self.y));

        if area.contains(&pos) {
            if entities.contains_key(&self.id) {
                return entities.remove(&self.id);
            }
        }

        None
    }

    fn destroy(&mut self, entities: &DashMap<usize, Entity>) -> Option<(usize, Entity)> {
        entities.remove(&self.id)
    }
}

impl Paint for Entity {
    fn colorize(&mut self, color: i32) {
        self.color = color;
    }
}

pub trait Create {
    fn spawn(
        x: f32,
        y: f32,
        radius: f32,
        color: i32,
        entities: &DashMap<usize, Entity>,
    ) -> Option<usize>;
}

pub trait Paint {
    fn colorize(&mut self, color: i32);
}

pub trait Eraser {
    fn erase(&mut self, area: Circle, entities: &DashMap<usize, Entity>)
    -> Option<(usize, Entity)>;

    fn destroy(&mut self, entities: &DashMap<usize, Entity>) -> Option<(usize, Entity)>;
}
