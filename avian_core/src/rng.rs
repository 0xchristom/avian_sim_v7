use rand_chacha::ChaCha8Rng;
use rand::{Rng, RngCore, SeedableRng};
use rand::distributions::{Distribution, Standard};
use rand::distributions::uniform::{SampleUniform, SampleRange};

pub struct SimRng(ChaCha8Rng);

impl SimRng {
    pub fn from_seed(seed: u64) -> Self {
        Self(ChaCha8Rng::seed_from_u64(seed))
    }

    /// POPRAWKA: Akceptuje Range (0..3) ORAZ RangeInclusive (0..=3)
    pub fn gen_range<T, R>(&mut self, range: R) -> T
    where
        T: SampleUniform,
        R: SampleRange<T>,
    {
        self.0.gen_range(range)
    }

    pub fn sample<D, T>(&mut self, dist: D) -> T
    where
        D: Distribution<T>,
    {
        self.0.sample(dist)
    }

    pub fn gen<T>(&mut self) -> T
    where
        Standard: Distribution<T>,
    {
        self.0.gen()
    }
}

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