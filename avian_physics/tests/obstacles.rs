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
