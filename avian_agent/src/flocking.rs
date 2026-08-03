//! Flocking / Boids (2.1).
//!
//! Boids is a continuous steering FORCE, NOT a behavior-tree branch (2.0b).
//! `steering()` computes a steering vector (separation + alignment + cohesion)
//! that `run_systems` SUMS onto whatever velocity the tree already selected —
//! exactly like head-bob overlays locomotion. Fleeing is the exception: while
//! Flee is active, boids steering is suppressed entirely (a fleeing pigeon
//! does not align with its flock).
//!
//! Neighbor lookups reuse the Phase-1 `spatial_grid.query_k_nearest` /
//! `query_radius` (avian_core::spatial). The grid is cleared/rebuilt once per
//! tick in `run_systems`; boids only reads from it, never writes. There is no
//! second neighbor-search mechanism.

use avian_core::calibration;
use avian_core::components::FSMState;
use nalgebra::Vector2;

#[derive(Clone, Copy, Debug)]
pub struct BoidWeights {
    pub separation: f64,
    pub alignment: f64,
    pub cohesion: f64,
}

pub fn default_weights() -> BoidWeights {
    BoidWeights {
        separation: calibration::BOID_SEPARATION_WEIGHT,
        alignment: calibration::BOID_ALIGNMENT_WEIGHT,
        cohesion: calibration::BOID_COHESION_WEIGHT,
    }
}

/// 2.1 state modulation: fleeing gets stronger separation (and its alignment/
/// cohesion are zeroed — though Fleeing is also fully suppressed upstream);
/// foraging drops cohesion to ZERO (Audit 4 §9.8: a foraging bird is influenced
/// by separation only — it is never pulled toward the flock centroid, so it
/// can search on its own instead of orbiting the group's center of mass).
pub fn weights_for_state(state: FSMState, base: &BoidWeights) -> BoidWeights {
    match state {
        FSMState::Foraging => BoidWeights {
            cohesion: 0.0,
            ..*base
        },
        FSMState::Fleeing => BoidWeights {
            separation: base.separation * 1.5,
            alignment: 0.0,
            cohesion: 0.0,
        },
        _ => *base,
    }
}

/// Compute the boids steering vector for an agent.
///
/// `neighbors` = `(pos, vel, dist)` triples; the caller guarantees the agent's
/// own entity is excluded (or has dist ~ 0, which we skip).
pub fn steering(
    pos: Vector2<f64>,
    neighbors: &[(Vector2<f64>, Vector2<f64>, f64)],
    weights: &BoidWeights,
) -> Vector2<f64> {
    let mut separation = Vector2::zeros();
    let mut cohesion_sum = Vector2::zeros();
    let mut alignment_sum = Vector2::zeros();
    let mut n = 0usize;

    for (npos, nvel, ndist) in neighbors {
        if *ndist < 1e-6 {
            continue; // self
        }
        n += 1;

        // Separation: push away within the avoidance radius, stronger when closer.
        if *ndist < calibration::BOID_SEPARATION_RADIUS_M {
            let away = (pos - npos) / *ndist;
            separation += away * (1.0 - ndist / calibration::BOID_SEPARATION_RADIUS_M);
        }

        cohesion_sum += npos;

        if nvel.norm() > 1e-6 {
            alignment_sum += nvel / nvel.norm();
        }
    }

    if n == 0 {
        return Vector2::zeros();
    }

    let cohesion = (cohesion_sum / n as f64 - pos) * weights.cohesion;
    let alignment = (alignment_sum / n as f64) * weights.alignment;
    let sep = separation * weights.separation;

    sep + alignment + cohesion
}

/// Count agents within `radius` of the given position (used by flock metrics).
pub fn neighbors_in_radius(pos: Vector2<f64>, radius: f64, positions: &[Vector2<f64>]) -> usize {
    positions
        .iter()
        .filter(|p| {
            let d = (**p - pos).norm();
            d <= radius && d > 1e-6
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_separation_dominant_close() {
        // Two neighbors on top of each other: separation must be non-trivial.
        let pos = Vector2::new(0.0, 0.0);
        let neighbors = vec![
            (Vector2::new(0.1, 0.0), Vector2::new(1.0, 0.0), 0.1),
            (Vector2::new(0.0, 0.1), Vector2::new(1.0, 0.0), 0.1),
        ];
        let s = steering(pos, &neighbors, &default_weights());
        assert!(s.norm() > 0.0, "expected non-zero steering");
        assert!(s.x < 0.0, "separation should push left/away");
    }

    #[test]
    fn test_no_neighbors_no_steering() {
        let s = steering(Vector2::zeros(), &[], &default_weights());
        assert_eq!(s, Vector2::zeros());
    }

    #[test]
    fn test_weights_modulation() {
        let base = default_weights();
        let foraging = weights_for_state(FSMState::Foraging, &base);
        assert!(foraging.cohesion < base.cohesion);
        let fleeing = weights_for_state(FSMState::Fleeing, &base);
        assert!(fleeing.separation > base.separation);
        assert_eq!(fleeing.alignment, 0.0);
        assert_eq!(fleeing.cohesion, 0.0);
    }
}
