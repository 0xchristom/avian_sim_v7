use avian_core::rng::SimRng;
use rand_distr::{Distribution, Normal};

pub enum SearchMode {
    Brownian,
    Levy,
    Directed,
}

pub fn levy_step(rng: &mut SimRng, mu: f64) -> f64 {
    let u: f64 = rng.gen();
    1.0 * (1.0 - u).powf(-1.0 / mu)
}

pub fn crw_direction(prev_heading: f64, rng: &mut SimRng, concentration: f64) -> f64 {
    let normal = Normal::new(0.0, 1.0 / concentration).unwrap();
    prev_heading + normal.sample(rng)
}

pub fn next_step(mode: SearchMode, prev_heading: f64, rng: &mut SimRng) -> (f64, f64) {
    match mode {
        SearchMode::Levy => {
            let dist = levy_step(rng, 2.0);
            let head = crw_direction(prev_heading, rng, 2.0);
            (dist, head)
        }
        SearchMode::Brownian => {
            let dist: f64 = rng.gen_range(0.1..0.5);
            let head: f64 = rng.gen_range(-std::f64::consts::PI..std::f64::consts::PI);
            (dist, head)
        }
        SearchMode::Directed => {
            let dist: f64 = rng.gen_range(1.0..5.0);
            (dist, prev_heading)
        }
    }
}
