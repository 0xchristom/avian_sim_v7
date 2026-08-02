//! SimRng — the simulation's single shared RNG.
//!
//! # 5.4 Parallelism RNG-substream DESIGN (design only — no implementation)
//!
//! __Problem__: all randomness currently flows through one shared, mutable
//! `SimRng`. Rayon-parallelizing any per-agent loop that calls `&mut sim.rng`
//! is impossible without a lock (serializes) or a data race (corrupts
//! determinism irreversibly — no post-hoc sort can undo random values already
//! assigned to the wrong agent during a race).
//!
//! __Prerequisite__ (must land BEFORE any Phase-2 feature consumes `sim.rng`
//! inside a to-be-parallelized loop): per-entity (or per-chunk) deterministic
//! RNG substreams. Two acceptable designs:
//!
//! 1. **Per-tick derived draws** — each tick, derive random values from a hash
//!    of `(global_seed, entity_id, frame_number)`. No shared mutable RNG in the
//!    parallel section at all.
//! 2. **Per-entity persistent substream** — each entity owns a `ChaCha8Rng`
//!    seeded at spawn from `(global_seed, entity_counter)`; persists across
//!    frames (swapped into the per-entity context).
//!
//! __Decision__: design (1) per-tick derived draws is preferred — it requires
//! no component addition, stays trivially deterministic, and needs no
//! serialization state beyond the existing seed. Design (2) is the fallback if
//! a phase later needs *correlated* multi-frame random streams.
//!
//! The parallel executor then: splits agent queries into chunks; each chunk
//! uses its own deterministic substream; merges outputs in entity order.
//! Determinism holds by construction, verified by the 7.1 serial-vs-parallel
//! byte-identical test (Sprint 6 ships the implementation).

use rand_chacha::ChaCha8Rng;
use rand::{Rng, RngCore, SeedableRng};
use rand::distributions::{Distribution, Standard};
use rand::distributions::uniform::SampleRange;
use serde::{Serialize, Deserialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct SimRng(ChaCha8Rng);

impl SimRng {
    pub fn from_seed(seed: u64) -> Self {
        Self(ChaCha8Rng::seed_from_u64(seed))
    }

    pub fn gen_range<T, R>(&mut self, range: R) -> T
    where
        T: rand::distributions::uniform::SampleUniform,
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
    fn next_u32(&mut self) -> u32 { self.0.next_u32() }
    fn next_u64(&mut self) -> u64 { self.0.next_u64() }
    fn fill_bytes(&mut self, dest: &mut [u8]) { self.0.fill_bytes(dest) }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.0.try_fill_bytes(dest)
    }
}