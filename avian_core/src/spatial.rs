use hecs::Entity;
use nalgebra::Vector2;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

pub struct SpatialHashGrid {
    cell_size: f64,
    cells: FxHashMap<u64, SmallVec<[Entity; 8]>>,
    /// Sprint 2 (Audit 5, B22): per-entity current cell key, so the grid can
    /// update incrementally — only entities that crossed a cell boundary are
    /// removed/reinserted, and unmoved entities keep their bucket slots.
    entity_cell: FxHashMap<Entity, u64>,
    /// Sprint 2 (Audit 5, B22): how many entities were actually moved by the
    /// most recent `update` pass (cell-boundary crossings). `0` when nobody
    /// moved; exposed for the 1%/10%/100% incremental-rebuild cost tests.
    pub last_update_moves: usize,
    /// Sprint 2 (Audit 5, B22): entities removed since the grid was built —
    /// retained so `remove` skips already-gone entities without a scan.
    removed_count: u64,
}

impl SpatialHashGrid {
    pub fn new(cell_size: f64) -> Self {
        Self {
            cell_size,
            cells: FxHashMap::default(),
            entity_cell: FxHashMap::default(),
            last_update_moves: 0,
            removed_count: 0,
        }
    }

    /// Sprint 2 (Audit 5, B22): pre-size the bucket map to avoid rehashing as
    /// the population grows (used by `Simulation::new`).
    pub fn with_capacity(cell_size: f64, expected_cells: usize) -> Self {
        let mut g = Self::new(cell_size);
        g.cells.reserve(expected_cells);
        g.entity_cell.reserve(expected_cells);
        g
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

    /// Force-insert (used for initial builds / checkpoint restores). Records the
    /// entity's cell for subsequent incremental updates.
    pub fn insert(&mut self, entity: Entity, pos: Vector2<f64>) {
        let key = self.hash(pos.x, pos.y);
        self.cells
            .entry(key)
            .or_insert_with(SmallVec::new)
            .push(entity);
        self.entity_cell.insert(entity, key);
    }

    /// Sprint 2 (Audit 5, B22): incremental cell update. Returns `true` if the
    /// entity crossed a cell boundary (and was moved in the buckets), `false`
    /// if it stayed in the same cell (bucket slot untouched). Callers use the
    /// aggregate `last_update_moves` to measure how much rebuild work happened.
    pub fn update(&mut self, entity: Entity, pos: Vector2<f64>) -> bool {
        let key = self.hash(pos.x, pos.y);
        match self.entity_cell.get(&entity) {
            Some(&prev) if prev == key => return false,
            Some(&prev) => {
                // Remove from the old bucket, preserving the relative order of
                // the remaining members (deterministic cell-scan order).
                if let Some(vec) = self.cells.get_mut(&prev) {
                    if let Some(idx) = vec.iter().position(|e| *e == entity) {
                        vec.remove(idx);
                    }
                    if vec.is_empty() {
                        self.cells.remove(&prev);
                    }
                }
            }
            None => {}
        }
        self.cells
            .entry(key)
            .or_insert_with(SmallVec::new)
            .push(entity);
        self.entity_cell.insert(entity, key);
        self.last_update_moves += 1;
        true
    }

    /// Sprint 2 (Audit 5, B22): drop an entity from the grid (despawned /
    /// consumed). No-op if already absent.
    pub fn remove(&mut self, entity: Entity) {
        if let Some(key) = self.entity_cell.remove(&entity) {
            if let Some(vec) = self.cells.get_mut(&key) {
                if let Some(idx) = vec.iter().position(|e| *e == entity) {
                    vec.remove(idx);
                }
                if vec.is_empty() {
                    self.cells.remove(&key);
                }
            }
            self.removed_count += 1;
        }
    }

    /// Sprint 2 (Audit 5): exact-radius query. Returns entities whose true
    /// position (via `pos_of`) lies within `radius` of `pos` — the caller must
    /// supply a position lookup because the grid stores only entity ids. The
    /// returned order is the deterministic grid cell scan order; sort by entity
    /// id when a stable order is required.
    pub fn query_radius_with<F>(&self, pos: Vector2<f64>, radius: f64, mut pos_of: F) -> Vec<Entity>
    where
        F: FnMut(Entity) -> Option<Vector2<f64>>,
    {
        let mut result = Vec::new();
        self.query_radius_into(pos, radius, &mut result, &mut pos_of);
        result
    }

    /// Sprint 2 (Audit 5): exact-radius query writing into a caller-owned
    /// scratch buffer (avoids a fresh allocation per call in hot systems).
    pub fn query_radius_into<F>(
        &self,
        pos: Vector2<f64>,
        radius: f64,
        out: &mut Vec<Entity>,
        pos_of: &mut F,
    ) where
        F: FnMut(Entity) -> Option<Vector2<f64>>,
    {
        out.clear();
        let radius_sq = radius * radius;
        let cell_radius = (radius / self.cell_size).ceil() as i64;

        for i in -cell_radius..=cell_radius {
            for j in -cell_radius..=cell_radius {
                let key = self.hash(
                    pos.x + i as f64 * self.cell_size,
                    pos.y + j as f64 * self.cell_size,
                );
                if let Some(entities) = self.cells.get(&key) {
                    for e in entities.iter().copied() {
                        if let Some(p) = pos_of(e) {
                            if (p - pos).norm_squared() <= radius_sq {
                                out.push(e);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Sprint 2 (Audit 5): exact-radius query backed by a positions map (the
    /// common agent/neighbor case).
    pub fn query_radius(
        &self,
        pos: Vector2<f64>,
        radius: f64,
        positions: &FxHashMap<Entity, Vector2<f64>>,
    ) -> Vec<Entity> {
        self.query_radius_with(pos, radius, |e| positions.get(&e).copied())
    }

    /// Query k-nearest entities within a bounded radius, EXACTLY filtered to
    /// the requested radius.
    /// `positions` is used only to compute distances for entities ALREADY found
    /// in the grid cells — it never scans the full entity set.
    /// `k=0` means "return all within radius".
    ///
    /// Sprint 2 (Audit 5): the previous implementation early-stopped on
    /// `candidates[k-1]` BEFORE sorting — `candidates[k-1]` is the k-th element
    /// in cell-scan order, not the k-th nearest, so the ring bound was unsound.
    /// The search now scans all rings within `radius`, filters by exact
    /// squared distance, then selects the top-k with `select_nth_unstable_by`
    /// and sorts deterministically (distance, then entity id). This never
    /// returns entities beyond `radius` and never depends on collection order.
    pub fn query_k_nearest(
        &self,
        pos: Vector2<f64>,
        k: usize,
        radius: f64,
        positions: &FxHashMap<Entity, Vector2<f64>>,
    ) -> Vec<(Entity, f64)> {
        let mut out = Vec::new();
        self.query_k_nearest_into(pos, k, radius, positions, &mut out);
        out
    }

    /// Sprint 2 (Audit 5): `query_k_nearest` writing into a caller-owned
    /// scratch buffer.
    pub fn query_k_nearest_into(
        &self,
        pos: Vector2<f64>,
        k: usize,
        radius: f64,
        positions: &FxHashMap<Entity, Vector2<f64>>,
        out: &mut Vec<(Entity, f64)>,
    ) {
        out.clear();
        let radius_sq = radius * radius;
        let cell_radius = (radius / self.cell_size).ceil() as i64;

        for i in -cell_radius..=cell_radius {
            for j in -cell_radius..=cell_radius {
                let key = self.hash(
                    pos.x + i as f64 * self.cell_size,
                    pos.y + j as f64 * self.cell_size,
                );
                if let Some(entities) = self.cells.get(&key) {
                    for e in entities.iter().copied() {
                        if let Some(p) = positions.get(&e) {
                            let dist_sq = (p - pos).norm_squared();
                            if dist_sq <= radius_sq {
                                out.push((e, dist_sq.sqrt()));
                            }
                        }
                    }
                }
            }
        }

        // Deterministic total order: distance, then entity id. `select_nth` only
        // guarantees the k-th element is in place; we re-sort the top-k so the
        // returned order is bit-stable across runs regardless of scan order.
        let cmp = |a: &(Entity, f64), b: &(Entity, f64)| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.to_bits().get().cmp(&b.0.to_bits().get()))
        };
        if k > 0 && out.len() > k {
            out.select_nth_unstable_by(k, cmp);
            out.truncate(k);
        }
        out.sort_by(cmp);
    }

    pub fn clear(&mut self) {
        self.cells.clear();
        self.entity_cell.clear();
        self.last_update_moves = 0;
        self.removed_count = 0;
    }

    /// Sprint 2 (Audit 5, B22): drop every indexed entity that is NOT a live
    /// key in `live` (e.g. agents despawned since the last tick). This keeps the
    /// incremental grid from accumulating ghosts — a full clear+reinsert would
    /// naturally discard them, so `sync_from` preserves that invariant without
    /// re-hashing the live population. Deterministic: the iteration order of
    /// `FxHashMap` is seed-independent, and removals only affect ghosts.
    pub fn sync_from(&mut self, live: &FxHashMap<Entity, Vector2<f64>>) {
        let mut ghost_keys: Vec<u64> = Vec::new();
        for (key, vec) in self.cells.iter_mut() {
            vec.retain(|e| live.contains_key(e));
            if vec.is_empty() {
                ghost_keys.push(*key);
            }
        }
        for key in ghost_keys {
            self.cells.remove(&key);
        }
        self.entity_cell.retain(|e, _| live.contains_key(e));
    }

    /// Sprint 2 (Audit 5, B22): number of entities currently indexed (live
    /// members across all buckets).
    pub fn len(&self) -> usize {
        self.cells.values().map(|v| v.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
            .query_radius(pos, radius, positions)
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
            (9.1, 9.2),
            (9.5, 10.3),
            (10.2, 9.8),
            (10.6, 10.9),
            (8.7, 11.0),
            (11.4, 9.1),
            (9.9, 8.8),
            (10.8, 10.1),
            (10.0, 10.0),
            (12.0, 12.0),
            (3.0, 3.0),
            (3.1, 3.2),
            (2.8, 3.4),
            (3.5, 2.9),
            (20.0, 5.0),
            (19.8, 5.2),
            (30.0, 30.0),
            (0.5, 0.5),
            (15.5, 15.5),
            (16.0, 14.9),
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

    // Sprint 2 (Audit 5): boundary radius tests — a neighbor in an adjacent
    // cell, a point exactly on the radius edge, and a point just outside the
    // circle. The query must never return an entity beyond the requested radius
    // (the old cell-scan returned everything in a cell, up to cell*√2 too far).
    #[test]
    fn query_radius_never_returns_beyond_radius() {
        let mut grid = SpatialHashGrid::new(2.0);
        let mut positions = FxHashMap::default();
        let mut world = hecs::World::new();
        // Neighbor cell: at (11.9, 10.0) vs query (10.0, 10.0) → 1.9 m away.
        // Inside the query cell but beyond 1.5 m? (10.0, 10.0) → (11.4, 10.0) is
        // 1.4 m (inside) — good. (11.99, 10.0) is ~2 m (outside).
        let e_in = world.spawn(());
        positions.insert(e_in, Vector2::new(11.4, 10.0));
        grid.insert(e_in, Vector2::new(11.4, 10.0));
        let e_out = world.spawn(());
        positions.insert(e_out, Vector2::new(11.99, 10.0));
        grid.insert(e_out, Vector2::new(11.99, 10.0));
        let e_edge = world.spawn(());
        positions.insert(e_edge, Vector2::new(11.5, 10.0));
        grid.insert(e_edge, Vector2::new(11.5, 10.0));

        let qp = Vector2::new(10.0, 10.0);
        let r1 = grid.query_radius(qp, 1.5, &positions);
        assert!(r1.contains(&e_in), "1.4 m neighbor must be inside r=1.5");
        assert!(
            r1.contains(&e_edge),
            "exactly-on-edge (1.5 m) must be inside"
        );
        assert!(
            !r1.contains(&e_out),
            "~2 m neighbor must NOT be returned for r=1.5"
        );

        // All three inside r=2.5.
        let r2 = grid.query_radius(qp, 2.5, &positions);
        assert_eq!(r2.len(), 3);
    }

    // Sprint 2 (Audit 5): `query_k_nearest` with a large k still respects the
    // exact radius (never over-returns), and the `_into` variants fill a
    // caller-owned buffer without extra allocations.
    #[test]
    fn k_nearest_respects_exact_radius_and_into_buffer() {
        let mut grid = SpatialHashGrid::new(2.0);
        let mut positions = FxHashMap::default();
        let mut world = hecs::World::new();
        for (x, y) in [
            (10.0, 10.0),
            (10.5, 10.0),
            (11.0, 10.0),
            (11.5, 10.0),
            (12.0, 10.0),
            (13.0, 10.0),
            (10.0, 14.0),
            (3.0, 3.0),
        ] {
            let e = world.spawn(());
            positions.insert(e, Vector2::new(x, y));
            grid.insert(e, Vector2::new(x, y));
        }
        let qp = Vector2::new(10.0, 10.0);
        // k = 100 (return all within 3 m): only points ≤ 3 m qualify.
        let all = grid.query_k_nearest(qp, 100, 3.0, &positions);
        assert!(
            all.iter().all(|(_, d)| *d <= 3.0 + 1e-12),
            "over-returned beyond radius"
        );
        assert_eq!(
            all.len(),
            6,
            "points at 0, 0.5, 1, 1.5, 2, 3 m are within 3 m"
        );

        // _into variant must produce identical results into a pre-owned buffer.
        let sentinel = world.spawn(());
        let mut buf: Vec<(Entity, f64)> = vec![(sentinel, -1.0)];
        grid.query_k_nearest_into(qp, 3, 3.0, &positions, &mut buf);
        assert_eq!(buf.len(), 3);
        let want = grid.query_k_nearest(qp, 3, 3.0, &positions);
        assert_eq!(buf, want);
    }

    // Sprint 2 (Audit 5): top-k matches brute-force over a larger random field,
    // deterministically stable under repeated calls.
    #[test]
    fn top_k_matches_brute_force_property() {
        let mut grid = SpatialHashGrid::new(2.0);
        let mut positions = FxHashMap::default();
        let mut world = hecs::World::new();
        let mut rng = crate::rng::SimRng::from_seed(2024);
        for _ in 0..120 {
            let e = world.spawn(());
            let p = Vector2::new(rng.gen_range(0.0..40.0), rng.gen_range(0.0..30.0));
            positions.insert(e, p);
            grid.insert(e, p);
        }
        let qp = Vector2::new(20.0, 15.0);
        let mut brute: Vec<(Entity, f64)> = positions
            .iter()
            .map(|(e, p)| (*e, (p - qp).norm()))
            .filter(|(_, d)| *d <= 7.0)
            .collect();
        brute.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.to_bits().get().cmp(&b.0.to_bits().get()))
        });
        for k in [0, 1, 3, 7, 40, 1000] {
            let got = grid.query_k_nearest(qp, k, 7.0, &positions);
            let want: Vec<(Entity, f64)> = if k == 0 {
                brute.clone()
            } else {
                brute.iter().take(k.min(brute.len())).cloned().collect()
            };
            assert_eq!(got, want, "k={k} mismatch vs brute force");
        }
    }

    // Sprint 2 (Audit 5, B22): incremental cell updates. An entity that stays
    // within the same cell is NOT re-bucketed (returns false, bucket untouched);
    // crossing a boundary moves it. `last_update_moves` counts only actual
    // boundary crossings, so rebuild cost tracks the moving fraction, not N.
    #[test]
    fn incremental_update_skips_unmoved_and_counts_moves() {
        let mut grid = SpatialHashGrid::with_capacity(2.0, 16);
        let mut positions = FxHashMap::default();
        let mut world = hecs::World::new();
        let es: Vec<Entity> = (0..10)
            .map(|i| {
                let e = world.spawn(());
                positions.insert(e, Vector2::new(i as f64 * 1.9, 5.0)); // 1.9 m apart in 2 m cells
                e
            })
            .collect();
        for e in &es {
            grid.insert(*e, positions[e]);
        }
        assert_eq!(grid.len(), 10);

        // Nobody moves: zero re-buckets, zero moves reported.
        grid.last_update_moves = 0;
        for e in &es {
            assert!(
                !grid.update(*e, positions[e]),
                "same-cell update must not re-bucket"
            );
        }
        assert_eq!(
            grid.last_update_moves, 0,
            "no cell crossings → zero rebuild work"
        );
        assert_eq!(grid.len(), 10);

        // One agent crosses a cell boundary (i=0 at x=0.0 → 3.0, cell 0→1).
        grid.last_update_moves = 0;
        positions.insert(es[0], Vector2::new(3.0, 5.0));
        assert!(
            grid.update(es[0], Vector2::new(3.0, 5.0)),
            "boundary crossing must re-bucket"
        );
        assert_eq!(grid.last_update_moves, 1, "exactly one move reported");
        assert_eq!(grid.len(), 10);
        // The moved agent is still queryable at its new cell.
        let hits = grid.query_radius(Vector2::new(3.0, 5.0), 0.1, &positions);
        assert_eq!(hits, vec![es[0]]);
    }

    // Sprint 2 (Audit 5, B22): rebuild work must scale with the fraction of
    // agents that actually move across cell boundaries — 1% moves → ~1% work,
    // 10% → ~10%, 100% → ~100%. `last_update_moves` after each pass equals the
    // number of boundary crossings.
    #[test]
    fn rebuild_cost_scales_with_moving_fraction() {
        let n = 200;
        let mut grid = SpatialHashGrid::with_capacity(2.0, 64);
        let mut positions = FxHashMap::default();
        let mut world = hecs::World::new();
        let es: Vec<Entity> = (0..n)
            .map(|i| {
                let e = world.spawn(());
                // 0.1 m apart: in a 2 m cell, up to 20 share a cell.
                let p = Vector2::new(i as f64 % 10.0 * 0.1, i as f64 / 10.0 * 0.1);
                positions.insert(e, p);
                e
            })
            .collect();
        for e in &es {
            grid.insert(*e, positions[e]);
        }
        assert_eq!(grid.len(), n);

        let mut move_frac = |frac: f64| {
            grid.last_update_moves = 0;
            let mut moved = 0;
            for (i, e) in es.iter().enumerate() {
                if (i as f64) < frac * n as f64 {
                    let p = *positions.get(e).unwrap() + Vector2::new(2.5, 0.0); // ≥ 1 cell away
                    positions.insert(*e, p);
                    if grid.update(*e, p) {
                        moved += 1;
                    }
                }
            }
            (moved, grid.last_update_moves)
        };

        let (moved_1pct, work_1pct) = move_frac(0.01);
        let (moved_10pct, work_10pct) = move_frac(0.10);
        let (moved_100pct, work_100pct) = move_frac(1.00);

        assert_eq!(moved_1pct, 2, "1% of 200 = 2 crossing agents");
        assert_eq!(work_1pct, 2, "work must equal crossings for 1%");
        assert_eq!(moved_10pct, 20, "10% of 200 = 20 crossing agents");
        assert_eq!(work_10pct, 20, "work must equal crossings for 10%");
        assert_eq!(moved_100pct, n, "all agents cross at 100%");
        assert_eq!(work_100pct, n, "work must equal crossings for 100%");
        assert!(
            work_10pct > work_1pct && work_100pct > work_10pct,
            "work must scale monotonically with the moving fraction"
        );
    }

    // Sprint 2 (Audit 5, B22): `sync_from` drops despawned entities without a
    // full clear+reinsert — live members keep their buckets, ghosts are gone,
    // and the grid is left consistent (no stale buckets, no stale cells).
    #[test]
    fn sync_from_drops_despawned_and_keeps_live() {
        let mut grid = SpatialHashGrid::with_capacity(2.0, 16);
        let mut positions = FxHashMap::default();
        let mut world = hecs::World::new();
        let es: Vec<Entity> = (0..8)
            .map(|i| {
                let e = world.spawn(());
                positions.insert(e, Vector2::new(i as f64 * 2.1, 5.0));
                e
            })
            .collect();
        for e in &es {
            grid.insert(*e, positions[e]);
        }
        assert_eq!(grid.len(), 8);

        // Three entities "die": removed from the live map but NOT via grid.remove.
        positions.remove(&es[1]);
        positions.remove(&es[3]);
        positions.remove(&es[7]);
        grid.sync_from(&positions);
        assert_eq!(grid.len(), 5, "ghosts must be dropped");
        for e in [&es[0], &es[2], &es[4], &es[5], &es[6]] {
            let hits = grid.query_radius(positions[e], 0.01, &positions);
            assert_eq!(hits, vec![*e], "live entity {e:?} must remain queryable");
        }
        // Ghost entities are not returned by queries even if probed.
        let ghosts = grid.query_radius(Vector2::new(3.1, 5.0), 0.5, &positions);
        assert!(ghosts.is_empty(), "removed entity must not be found");
    }

    // Sprint 2 (Audit 5, B22): incremental updates must produce the SAME query
    // results as a full clear+reinsert for the same positions — the incremental
    // path is purely a rebuild-cost optimization, never a behavior change.
    #[test]
    fn incremental_and_full_rebuild_agree() {
        let n = 150;
        let mut rng = crate::rng::SimRng::from_seed(77);
        let mut positions = FxHashMap::default();
        let mut world = hecs::World::new();
        let es: Vec<Entity> = (0..n)
            .map(|_| {
                let e = world.spawn(());
                positions.insert(
                    e,
                    Vector2::new(rng.gen_range(0.0..40.0), rng.gen_range(0.0..30.0)),
                );
                e
            })
            .collect();

        // Reference: full rebuild.
        let mut full = SpatialHashGrid::with_capacity(2.0, 64);
        for e in &es {
            full.insert(*e, positions[e]);
        }

        // Incremental: insert then nudge every entity across cells and back.
        let mut incr = SpatialHashGrid::with_capacity(2.0, 64);
        for e in &es {
            incr.insert(*e, positions[e]);
        }
        for e in &es {
            let p = positions[e] + Vector2::new(2.6, 1.7);
            positions.insert(*e, p);
            incr.update(*e, p);
        }
        for e in &es {
            let p = positions[e] - Vector2::new(2.6, 1.7);
            positions.insert(*e, p);
            incr.update(*e, p);
        }

        for q in [(20.0, 15.0), (5.0, 5.0), (35.0, 25.0)] {
            for k in [0, 3, 10, 100] {
                let qp = Vector2::new(q.0, q.1);
                let a = full.query_k_nearest(qp, k, 7.0, &positions);
                let b = incr.query_k_nearest(qp, k, 7.0, &positions);
                assert_eq!(a, b, "incremental vs full rebuild diverge at q={q:?} k={k}");
            }
        }
    }
}
