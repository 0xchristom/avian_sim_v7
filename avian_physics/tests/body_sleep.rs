//! Sprint 2 (Audit 5, B10): the simulation must not force-wake sleeping/idle
//! bodies every tick. `set_linvel_if_changed` skips the write (and the wake-up
//! it implies) when the requested velocity has not materially changed, so
//! roosting agents stop producing per-tick wake-ups.

use avian_physics::PhysicsWorld;
use nalgebra::Vector2;

// Rapier 0.18 dropped the global sleep_threshold (IntegrationParameters no
// longer has it); bodies sleep via explicit `sleep()`. The B10 guarantee under
// test is the physics helper: `set_linvel_if_changed` must not write a
// velocity (and therefore must not force a wake-up) when the requested
// velocity is materially unchanged — roosting/idle agents stop hammering the
// solver every tick.

#[test]
fn unchanged_velocity_does_not_wake_body() {
    let mut world = PhysicsWorld::new();
    let body = world.spawn_agent_body(Vector2::new(5.0, 5.0), 0.3);

    // Put the body to sleep explicitly (the state a roosting agent reaches).
    let rb = world.get_body_mut(body).unwrap();
    rb.sleep();
    let rb = world.get_body(body).unwrap();
    assert!(rb.is_sleeping(), "body should be asleep after sleep()");

    // Requesting the SAME (zero) velocity must NOT wake it and must report
    // "not touched".
    let touched = world.set_linvel_if_changed(body, Vector2::new(0.0, 0.0), 0.01);
    assert!(!touched, "no-op velocity write must be skipped");
    let rb = world.get_body(body).unwrap();
    assert!(
        rb.is_sleeping(),
        "body must stay asleep when velocity is unchanged"
    );
}

#[test]
fn changed_velocity_wakes_sleeping_body() {
    let mut world = PhysicsWorld::new();
    let body = world.spawn_agent_body(Vector2::new(5.0, 5.0), 0.3);

    let rb = world.get_body_mut(body).unwrap();
    rb.sleep();
    let rb = world.get_body(body).unwrap();
    assert!(rb.is_sleeping(), "body should be asleep after sleep()");

    // A materially different velocity must wake it and be applied.
    let touched = world.set_linvel_if_changed(body, Vector2::new(5.0, 0.0), 0.01);
    assert!(touched, "a real velocity change must be written");
    let rb = world.get_body(body).unwrap();
    assert!(!rb.is_sleeping(), "a moving body must be awake");
    assert!((rb.linvel().x - 5.0).abs() < 1e-6, "velocity must be applied");
}

#[test]
fn repeated_unchanged_writes_are_skipped_after_awake() {
    let mut world = PhysicsWorld::new();
    let body = world.spawn_agent_body(Vector2::new(5.0, 5.0), 0.3);

    // First write is a real change → touched.
    let first = world.set_linvel_if_changed(body, Vector2::new(3.0, 0.0), 0.01);
    assert!(first, "first write must apply");

    // Same target again → no-op, no wake-up re-fire.
    let second = world.set_linvel_if_changed(body, Vector2::new(3.0, 0.0), 0.01);
    assert!(!second, "repeated identical write must be skipped");

    // Small drift within eps → still skipped.
    let drift = world.set_linvel_if_changed(body, Vector2::new(3.0 + 0.005, 0.0), 0.01);
    assert!(!drift, "sub-eps drift must be skipped");
}

