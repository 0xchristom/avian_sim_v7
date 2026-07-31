use hashbrown::HashMap;
use smallvec::SmallVec;
use hecs::Entity;
use nalgebra::Vector2;

pub struct SpatialHashGrid {
    cell_size: f64,
    cells: HashMap<u64, SmallVec<[Entity; 8]>>,
}

impl SpatialHashGrid {
    pub fn new(cell_size: f64) -> Self {
        Self {
            cell_size,
            cells: HashMap::new(),
        }
    }

    fn hash(&self, x: f64, y: f64) -> u64 {
        let cx = (x / self.cell_size).floor() as i64;
        let cy = (y / self.cell_size).floor() as i64;
        ((cx as u64) << 32) | (cy as u64 & 0xFFFFFFFF)
    }

    pub fn insert(&mut self, entity: Entity, pos: Vector2<f64>) {
        let key = self.hash(pos.x, pos.y);
        self.cells.entry(key).or_insert_with(SmallVec::new).push(entity);
    }

    pub fn query_radius(&self, pos: Vector2<f64>, radius: f64) -> Vec<Entity> {
        let mut result = Vec::new();
        let cell_radius = (radius / self.cell_size).ceil() as i64;

        for i in -cell_radius..=cell_radius {
            for j in -cell_radius..=cell_radius {
                let key = self.hash(pos.x + i as f64 * self.cell_size, pos.y + j as f64 * self.cell_size);
                if let Some(entities) = self.cells.get(&key) {
                    result.extend(entities.iter().copied());
                }
            }
        }
        result
    }

    pub fn query_k_nearest(&self, pos: Vector2<f64>, k: usize, positions: &HashMap<Entity, Vector2<f64>>) -> Vec<(Entity, f64)> {
        let mut candidates: Vec<(Entity, f64)> = self.query_radius(pos, 10.0)
            .into_iter()
            .filter_map(|e| {
                positions.get(&e).map(|p| {
                    let dist = (p - pos).norm();
                    (e, dist)
                })
            })
            .collect();

        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(k);
        candidates
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }
}
