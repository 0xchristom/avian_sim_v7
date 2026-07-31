use rand_chacha::ChaCha8Rng;
use rand::{Rng, RngCore, SeedableRng};
use rand::distributions::{Distribution, Standard};
use std::ops::Range;

pub struct SimRng(ChaCha8Rng);

impl SimRng {
    pub fn from_seed(seed: u64) -> Self {
        Self(ChaCha8Rng::seed_from_u64(seed))
    }

    pub fn gen_range<T>(&mut self, range: Range<T>) -> T
    where
        T: PartialOrd + rand::distributions::uniform::SampleUniform,
        Standard: Distribution<T>,
    {
        self.0.gen_range(range)
    }

    pub fn sample<D: Distribution<T>, T>(&mut self, dist: D) -> T {
        self.0.sample(dist)
    }

    pub fn gen<T>(&mut self) -> T
    where
        Standard: Distribution<T>,
    {
        self.0.gen()
    }
}

// Implementacja RngCore, aby SimRng mogło być używane bezpośrednio w rand_distr
impl RngCore for SimRng {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.0.try_fill_bytes(dest)
    }
}