pub struct TimeStep {
    pub dt: f64,
    pub accumulator: f64,
}

impl TimeStep {
    pub fn new(dt: f64) -> Self {
        Self { dt, accumulator: 0.0 }
    }

    pub fn step(&mut self, external_delta: f64) -> bool {
        self.accumulator += external_delta;
        if self.accumulator >= self.dt {
            self.accumulator -= self.dt;
            true
        } else {
            false
        }
    }
}

pub struct SimulationTime {
    pub dt: f64,
    pub frame: u32,
    pub time_us: u64,
    pub accumulator: f64,
}

impl SimulationTime {
    pub fn new(dt: f64) -> Self {
        Self { dt, frame: 0, time_us: 0, accumulator: 0.0 }
    }

    pub fn tick(&mut self) {
        self.accumulator += self.dt;
    }

    pub fn consume_tick(&mut self) -> bool {
        if self.accumulator >= self.dt {
            self.accumulator -= self.dt;
            true
        } else {
            false
        }
    }
}
