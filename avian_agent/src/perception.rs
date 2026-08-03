use hecs::Entity;
use nalgebra::Vector2;

/// Wrap a raw angle difference into `[-π, π]`. `rem_euclid` is REQUIRED (not
/// Rust's float `%`, which keeps the dividend's sign): `(x + π) % 2π` yields
/// values outside [-π, π] whenever `x < -π`, and the FOV gate then rejects
/// targets that are actually right in front of the pigeon.
pub fn normalize_angle_relative(angle: f64) -> f64 {
    (angle + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}

pub fn cone_cast<F: Fn(&Vector2<f64>, f64) -> bool>(
    origin: Vector2<f64>,
    heading: f64,
    fov: f64,
    max_dist: f64,
    targets: &[(Entity, Vector2<f64>)],
    occluded: F,
) -> Vec<(Entity, Vector2<f64>, f64)> {
    let mut visible = Vec::new();
    let half_fov = fov.to_radians() / 2.0;

    for (entity, pos) in targets {
        let dir = *pos - origin;
        let dist = dir.norm();
        if dist > max_dist || dist < 1e-6 {
            continue;
        }

        let angle = dir.y.atan2(dir.x) - heading;
        let normalized_angle = normalize_angle_relative(angle);

        if normalized_angle.abs() <= half_fov {
            // 4.3: line-of-sight occlusion — a wall or building on the sight
            // line hides the target even inside the FOV cone.
            if occluded(pos, dist) {
                continue;
            }
            let res = 1.0 / (1.0 + 0.1 * normalized_angle.abs().to_degrees());
            visible.push((*entity, *pos, res));
        }
    }
    visible
}

pub fn local_enhancement_score(neighbor_score: f64, threshold: f64, k: f64) -> f64 {
    1.0 / (1.0 + (-k * (neighbor_score - threshold)).exp())
}
