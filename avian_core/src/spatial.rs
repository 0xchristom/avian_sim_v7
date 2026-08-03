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
    ///
    /// Implementation note: searches cell rings expanding outward and stops as
    /// soon as the next ring's inner boundary lies beyond the k-th nearest
    /// distance so far. For dense flocks this touches only a handful of cells
    /// instead of the whole vision disc. The result is EXACTLY the same set and
    /// order as a full-disc scan + total-order sort (the final sort is the
    /// determinism anchor, so collection order never matters).
    pub fn query_k_nearest(
        &self,
        pos: Vector2<f64>,
        k: usize,
        radius: f64,
        positions: &FxHashMap<Entity, Vector2<f64>>,
    ) -> Vec<(Entity, f64)> {
        let mut candidates: Vec<(Entity, f64)> = Vec::new();
        let cell_radius = (radius / self.cell_size).ceil() as i64;
        let mut ring = 0i64;

        loop {
            if ring > cell_radius {
                break;
            }
            // Min possible distance of any entity in this ring's cells (cells
            // at Chebyshev distance `ring` have their nearest edge at least
            // (ring-1)*cell from the query cell center).
            if k > 0 && candidates.len() > k {
                let kth = candidates[k - 1].1;
                if (ring as f64 - 1.0) * self.cell_size > kth {
                    break;
                }
            }
            if ring == 0 {
                let key = self.hash(pos.x, pos.y);
                if let Some(entities) = self.cells.get(&key) {
                    for e in entities.iter().copied() {
                        if let Some(p) = positions.get(&e) {
                            candidates.push((e, (p - pos).norm()));
                        }
                    }
                }
            } else {
                for i in -ring..=ring {
                    for j in -ring..=ring {
                        if i.abs() != ring && j.abs() != ring {
                            continue;
                        }
                        let key =
                            self.hash(pos.x + i as f64 * self.cell_size, pos.y + j as f64 * self.cell_size);
                        if let Some(entities) = self.cells.get(&key) {
                            for e in entities.iter().copied() {
                                if let Some(p) = positions.get(&e) {
                                    candidates.push((e, (p - pos).norm()));
                                }
                            }
                        }
                    }
                }
            }
            ring += 1;
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference implementation: the pre-optimization full-disc scan. Must
    /// return an IDENTICAL (set, order) as the ring-expanding search for any
    /// query, so the caching/optimization never changes trajectories.
    fn reference_query_k_nearest(
        grid: &SpatialHashGrid,
        pos: Vector2<f64>,
        k: usize,
        radius: f64,
        positions: &FxHashMap<Entity, Vector2<f64>>,
    ) -> Vec<(Entity, f64)> {
        let mut candidates: Vec<(Entity, f64)> = grid
            .query_radius(pos, radius)
            .into_iter()
            .filter_map(|e| {
                positions.get(&e).map(|p| {
                    let dist = (p - pos).norm();
                    (e, dist)
                })
            })
            .collect();
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

    #[test]
    fn ring_search_matches_full_disc_scan() {
        let mut grid = SpatialHashGrid::new(2.0);
        let mut positions = FxHashMap::default();
        let mut world = hecs::World::new();
        // Dense cluster around (10,10) plus scattered points, so some queries
        // terminate early while others (near the scatter) must scan far.
        let pts = [
            (9.1, 9.2), (9.5, 10.3), (10.2, 9.8), (10.6, 10.9), (8.7, 11.0),
            (11.4, 9.1), (9.9, 8.8), (10.8, 10.1), (10.0, 10.0), (12.0, 12.0),
            (3.0, 3.0), (3.1, 3.2), (2.8, 3.4), (3.5, 2.9), (20.0, 5.0),
            (19.8, 5.2), (30.0, 30.0), (0.5, 0.5), (15.5, 15.5), (16.0, 14.9),
        ];
        for (x, y) in pts {
            let e = world.spawn(());
            let p = Vector2::new(x, y);
            positions.insert(e, p);
            grid.insert(e, p);
        }
        for radius in [1.0, 2.0, 3.5, 6.0, 15.0] {
            for k in [0, 1, 3, 7, 100] {
                for q in [(10.0, 10.0), (3.0, 3.0), (20.0, 5.0), (7.0, 7.0)] {
                    let qp = Vector2::new(q.0, q.1);
                    let got = grid.query_k_nearest(qp, k, radius, &positions);
                    let want = reference_query_k_nearest(&grid, qp, k, radius, &positions);
                    assert_eq!(
                        got.len(),
                        want.len(),
                        "len mismatch r={radius} k={k} q={q:?}"
                    );
                    for (a, b) in got.iter().zip(want.iter()) {
                        assert_eq!(a.0, b.0, "entity mismatch r={radius} k={k} q={q:?}");
                        assert!(
                            (a.1 - b.1).abs() < 1e-12,
                            "dist mismatch r={radius} k={k} q={q:?}: {} vs {}",
                            a.1,
                            b.1
                        );
                    }
                }
            }
        }
    }
}