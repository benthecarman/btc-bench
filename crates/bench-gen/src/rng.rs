//! Deterministic RNG: xorshift64*. Implements rand_core so secp256k1
//! key generation can consume it without pulling in a bigger RNG stack.

use rand_core::{CryptoRng, RngCore};

#[derive(Clone)]
pub struct SeededRng {
    state: u64,
}

impl SeededRng {
    /// Seed 0 is mapped to a nonzero constant; xorshift cannot escape 0.
    pub fn new(seed: u64) -> Self {
        SeededRng {
            state: seed ^ 0x9E3779B97F4A7C15,
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform in [0, n).
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0);
        // Rejection sampling keeps the distribution exact.
        let zone = u64::MAX - (u64::MAX % n) - 1;
        loop {
            let v = self.next_u64();
            if v <= zone {
                return v % n;
            }
        }
    }

    pub fn range(&mut self, lo: u64, hi_inclusive: u64) -> u64 {
        lo + self.below(hi_inclusive - lo + 1)
    }

    pub fn bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    pub fn bytes(&mut self, out: &mut [u8]) {
        let mut i = 0;
        while i + 8 <= out.len() {
            out[i..i + 8].copy_from_slice(&self.next_u64().to_le_bytes());
            i += 8;
        }
        if i < out.len() {
            let n = out.len() - i;
            let tail = self.next_u64().to_le_bytes();
            out[i..].copy_from_slice(&tail[..n]);
        }
    }
}

impl RngCore for SeededRng {
    fn next_u32(&mut self) -> u32 {
        SeededRng::next_u32(self)
    }
    fn next_u64(&mut self) -> u64 {
        SeededRng::next_u64(self)
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.bytes(dest)
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.bytes(dest);
        Ok(())
    }
}

impl CryptoRng for SeededRng {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let mut a = SeededRng::new(42);
        let mut b = SeededRng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn zero_seed_works() {
        let mut r = SeededRng::new(0);
        assert_ne!(r.next_u64(), 0);
    }

    #[test]
    fn below_stays_in_range() {
        let mut r = SeededRng::new(7);
        for _ in 0..1000 {
            assert!(r.below(5) < 5);
        }
    }
}
