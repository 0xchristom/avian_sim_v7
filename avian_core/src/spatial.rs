use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use hecs::Entity;
use nalgebra::Vector2;

pub struct SpatialHashGrid {
    cell_size: f64,
    cells: FxHashMap<u64, SmallVec<[Entity; 8]>>,
}

impl SpatialHashGrid {
    pub fn new(cell_size: f64) -> Self {
        Self {
            cell_size,
            cells: FxHashMap::default(),
        }
    }

    fn hash(&self, x: f64, y: f64) -> u64 {
        // Fix #9: Handle negative coordinates correctly.
        // Using i64→u64 cast causes hash collisions for negative coords.
        // Remap to positive space with an offset, then pack.
        let cx = (x / self.cell_size).floor() as i64;
        let cy = (y / self.cell_size).floor() as i64;
        // Offset by i32::MAX to make all coordinates non-negative before packing.
        let ox = (cx as i64).wrapping_add(i32::MAX as i64);
        let oy = (cy as i64).wrapping_add(i32::MAX as i64);
        ((ox as u64) << 32) | (oy as u64 & 0xFFFFFFFF)
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

    /// Query k-nearest entities within a bounded radius.
    /// `positions` is used only to compute distances for entities ALREADY found
    /// in the grid cells — it never scans the full entity set.
    /// `k=0` means "return all".
    pub fn query_k_nearest(
        &self,
        pos: Vector2<f64>,
        k: usize,
        radius: f64,
        positions: &FxHashMap<Entity, Vector2<f64>>,
    ) -> Vec<(Entity, f64)> {
        let mut candidates: Vec<(Entity, f64)> = self
            .query_radius(pos, radius)
            .into_iter()
            .filter_map(|e| {
                positions.get(&e).map(|p| {
                    let dist = (p - pos).norm();
                    (e, dist)
                })
            })
            .collect();

        // Deterministic sort (tie-broken by entity id) so that parallel
        // runs of the same seed produce identical ordering.
        candidates.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.to_bits().get().cmp(&b.0.to_bits().get()))
        });

        if k > 0 && candidates.len() > k {
            candidates.truncate(k);
        }
        candidates
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }
}