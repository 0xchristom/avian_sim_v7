//! 4.3 obstacle physics tests: static box colliders block dynamic bodies, and
//! the line-of-sight raycast (`cast_ray_to_static`) treats walls + obstacles
//! as occluders while ignoring dynamic bodies.

use avian_physics::PhysicsWorld;
use nalgebra::Vector2;

/// A dynamic body pushed toward a fixed box obstacle must NOT pass through it.
#[test]
fn static_obstacle_stops_dynamic_body() {
    let mut world = PhysicsWorld::new();
    // Vertical wall spanning the full test arena height.
    world.add_obstacle(Vector2::new(8.0, 0.0), Vector2::new(10.0, 10.0));
    let body = world.spawn_agent_body(Vector2::new(4.0, 5.0), 0.3);

    // Push straight +x into the wall at walking pace for 3 simulated seconds.
    for _ in 0..(120 * 3) {
        let rb = world.get_body_mut(body).unwrap();
        rb.set_linvel(Vector2::new(5.0, 0.0), true);
        world.step();
    }

    let pos = world.get_body(body).unwrap().translation();
    assert!(
        pos.x < 8.5,
        "body pushed through the obstacle: x = {:.2} (obstacle starts at 8.0)",
        pos.x
    );
}

/// Control for the above: the same body with NO obstacle crosses the region.
#[test]
fn body_crosses_without_obstacle() {
    let mut world = PhysicsWorld::new();
    let body = world.spawn_agent_body(Vector2::new(4.0, 5.0), 0.3);

    for _ in 0..(120 * 3) {
        let rb = world.get_body_mut(body).unwrap();
        rb.set_linvel(Vector2::new(5.0, 0.0), true);
        world.step();
    }

    let pos = world.get_body(body).unwrap().translation();
    assert!(pos.x > 10.0, "control body should have passed x=10, got x = {:.2}", pos.x);
}

/// `cast_ray_to_static` reports a hit when a static obstacle lies on the
/// segment, and None when the path is clear.
#[test]
fn line_of_sight_respects_obstacles() {
    let mut world = PhysicsWorld::new();
    world.add_obstacle(Vector2::new(8.0, 0.0), Vector2::new(10.0, 10.0));

    // From (4,5) toward (14,5): the box sits between, so toi lands inside it.
    let hit = world.cast_ray_to_static(Vector2::new(4.0, 5.0), Vector2::new(10.0, 0.0), 1.0);
    assert!(
        hit.is_some_and(|t| t < 1.0 - 1e-4),
        "expected a blocking hit between (4,5) and (14,5), got {hit:?}"
    );

    // From (4,5) toward (6,5): clear path, no static geometry on the way.
    let clear = world.cast_ray_to_static(Vector2::new(4.0, 5.0), Vector2::new(2.0, 0.0), 1.0);
    assert!(clear.is_none(), "expected a clear LOS to (6,5), got {clear:?}");
}

/// Walls added via `add_wall` are fixed geometry too and block sight lines.
#[test]
fn line_of_sight_respects_walls() {
    let mut world = PhysicsWorld::new();
    world.add_wall(Vector2::new(6.0, 0.0), Vector2::new(6.0, 10.0));

    // Ray across the wall at y=5: wall at x=6, origin at x=2 → toi = 4/8 = 0.5.
    let hit = world.cast_ray_to_static(Vector2::new(2.0, 5.0), Vector2::new(8.0, 0.0), 1.0);
    assert!(hit.is_some_and(|t| t < 1.0 - 1e-4), "wall should occlude, got {hit:?}");

    // Parallel to the wall (at y=12, above its y=0..10 span): never intersects.
    let clear = world.cast_ray_to_static(Vector2::new(2.0, 12.0), Vector2::new(8.0, 0.0), 1.0);
    assert!(clear.is_none(), "parallel ray should not hit the wall, got {clear:?}");
}

/// Dynamic bodies are never occluders — only static geometry blocks vision.
#[test]
fn dynamic_bodies_do_not_block_sight() {
    let mut world = PhysicsWorld::new();
    // A moving agent parked between origin and target must not occlude.
    world.spawn_agent_body(Vector2::new(6.0, 5.0), 0.3);

    let hit = world.cast_ray_to_static(Vector2::new(4.0, 5.0), Vector2::new(8.0, 0.0), 1.0);
    assert!(hit.is_none(), "dynamic agent should not block LOS, got {hit:?}");
}

/// Sprint 2 (Audit 5, B9): the coarse static broad-phase culls open-space rays
/// BEFORE Rapier — the authoritative-raycast counter must stay at 0 for rays
/// that never approach static geometry, and the result is still `None`.
#[test]
fn raycast_counter_stays_zero_for_open_space() {
    let mut world = PhysicsWorld::new();
    world.add_obstacle(Vector2::new(8.0, 0.0), Vector2::new(10.0, 10.0));
    world.reset_raycast_count();

    // Far away from the obstacle — coarse broad-phase clears it.
    let clear = world.cast_ray_to_static(Vector2::new(4.0, 5.0), Vector2::new(2.0, 0.0), 1.0);
    assert!(clear.is_none());
    assert_eq!(world.los_raycast_count(), 0, "open-space ray must not reach Rapier");
}

/// Sprint 2 (Audit 5, B9/B24): a ray that genuinely approaches a static
/// obstacle DOES issue the authoritative Raycast, and the counter reflects it.
#[test]
fn raycast_counter_counts_blocked_rays() {
    let mut world = PhysicsWorld::new();
    world.add_obstacle(Vector2::new(8.0, 0.0), Vector2::new(10.0, 10.0));
    world.reset_raycast_count();

    let hit = world.cast_ray_to_static(Vector2::new(4.0, 5.0), Vector2::new(10.0, 0.0), 1.0);
    assert!(hit.is_some(), "obstacle on the segment must be detected");
    assert_eq!(world.los_raycast_count(), 1, "blocked ray must reach Rapier exactly once");

    // After reset, a blocked ray counts again.
    world.reset_raycast_count();
    world.cast_ray_to_static(Vector2::new(4.0, 5.0), Vector2::new(10.0, 0.0), 1.0);
    assert_eq!(world.los_raycast_count(), 1);
}

/// Sprint 2 (Audit 5, B9): a ray STARTING inside a static collider must still
/// be reported (coarse broad-phase must forward it to the authoritative query,
/// not cull it). The agent is inside the box (8..10 × 0..10), aimed out through
/// the wall at x=8.
#[test]
fn ray_starting_inside_obstacle_is_detected() {
    let mut world = PhysicsWorld::new();
    world.add_obstacle(Vector2::new(8.0, 0.0), Vector2::new(10.0, 10.0));

    let hit = world.cast_ray_to_static(Vector2::new(9.0, 5.0), Vector2::new(-2.0, 0.0), 1.0);
    assert!(hit.is_some(), "ray leaving an obstacle must still report the wall, got {hit:?}");
}

/// Sprint 2 (Audit 5, B9): a ray ENDING just before a static collider is clear;
/// the coarse broad-phase must not over-occlude. Segment ends at x=7.99, box
/// starts at x=8.
#[test]
fn ray_ending_before_obstacle_is_clear() {
    let mut world = PhysicsWorld::new();
    world.add_obstacle(Vector2::new(8.0, 0.0), Vector2::new(10.0, 10.0));

    let clear = world.cast_ray_to_static(Vector2::new(4.0, 5.0), Vector2::new(3.99, 0.0), 1.0);
    assert!(clear.is_none(), "segment stopping before the box must be clear, got {clear:?}");
}

/// Sprint 2 (Audit 5, B9): a ray GRAZING the obstacle's edge (passing within a
/// tiny margin but not through it) must not be reported as blocked — the coarse
/// broad-phase is conservative and forwards the near-miss to Rapier, which
/// returns None.
#[test]
fn ray_grazing_obstacle_edge_is_clear() {
    let mut world = PhysicsWorld::new();
    // Box from y=5.0 to y=6.0. Ray at y=6.2 passes just above the top edge.
    world.add_obstacle(Vector2::new(4.0, 5.0), Vector2::new(10.0, 6.0));

    let clear = world.cast_ray_to_static(Vector2::new(2.0, 6.2), Vector2::new(10.0, 0.0), 1.0);
    assert!(clear.is_none(), "grazing ray must be clear, got {clear:?}");
}

/// Sprint 2 (Audit 5, B9): the broad-phase must be rebuilt on checkpoint
/// restore (`from_state`) so a restored world culls LOS raycasts identically.
#[test]
fn broadphase_survives_checkpoint_roundtrip() {
    let mut world = PhysicsWorld::new();
    world.add_obstacle(Vector2::new(8.0, 0.0), Vector2::new(10.0, 10.0));
    world.add_wall(Vector2::new(6.0, 0.0), Vector2::new(6.0, 10.0));
    let state = world.to_state();
    let restored = PhysicsWorld::from_state(state);
    assert_eq!(restored.static_broadphase.len(), 2, "both AABBs survive roundtrip");

    // A blocked ray is still detected after restore.
    let hit = restored.cast_ray_to_static(Vector2::new(4.0, 5.0), Vector2::new(10.0, 0.0), 1.0);
    assert!(hit.is_some(), "restored world must still detect the obstacle, got {hit:?}");
    // An open-space ray is still culled (counter stays 0).
    let clear = restored.cast_ray_to_static(Vector2::new(4.0, 5.0), Vector2::new(1.0, 0.0), 1.0);
    assert!(clear.is_none());
    assert_eq!(restored.los_raycast_count(), 1, "only the blocked ray reaches Rapier");
}
