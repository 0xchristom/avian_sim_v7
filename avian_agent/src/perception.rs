use nalgebra::Vector2;
use hecs::Entity;

pub fn cone_cast(
    origin: Vector2<f64>,
    heading: f64,
    fov: f64,
    max_dist: f64,
    targets: &[(Entity, Vector2<f64>)],
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
        let normalized_angle = ((angle + std::f64::consts::PI) % (2.0 * std::f64::consts::PI)) - std::f64::consts::PI;
        
        if normalized_angle.abs() <= half_fov {
            let res = 1.0 / (1.0 + 0.1 * normalized_angle.abs().to_degrees());
            visible.push((*entity, *pos, res));
        }
    }
    visible
}

pub fn local_enhancement_score(
    neighbor_score: f64,
    threshold: f64,
    k: f64
) -> f64 {
    1.0 / (1.0 + (-k * (neighbor_score - threshold)).exp())
}
