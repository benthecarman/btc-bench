//! Labeled key material, one set per task, generated from the task's RNG
//! stream. Legacy/segwit tasks show 33-byte compressed keys; taproot
//! tasks show 32-byte x-only keys.

use bitcoin::key::{PublicKey, XOnlyPublicKey};
use secp256k1::{Secp256k1, SecretKey};

use crate::rng::SeededRng;

pub const LABELS: [&str; 12] = [
    "Alice", "Bob", "Carol", "Dave", "Erin", "Frank", "Grace", "Heidi", "Ivan", "Judy", "Ken",
    "Laura",
];

/// A task's keys: parallel arrays of real curve points.
#[derive(Clone, Debug)]
pub struct KeySet {
    pub labels: Vec<String>,
    /// 33-byte compressed, hex — for legacy and segwit tasks.
    pub compressed: Vec<String>,
    /// 32-byte x-only, hex — for taproot tasks.
    pub xonly: Vec<String>,
}

impl KeySet {
    pub fn len(&self) -> usize {
        self.labels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    pub fn label(&self, i: usize) -> &str {
        &self.labels[i]
    }
}

pub fn generate(rng: &mut SeededRng, count: usize) -> KeySet {
    assert!(count <= LABELS.len(), "task atom count exceeds label pool");
    let secp = Secp256k1::new();
    let mut compressed = Vec::with_capacity(count);
    let mut xonly = Vec::with_capacity(count);
    for _ in 0..count {
        let sk = SecretKey::new(&mut *rng);
        let pk = PublicKey::from(secp256k1::PublicKey::from_secret_key(&secp, &sk));
        let xo = XOnlyPublicKey::from(pk);
        compressed.push(hex(&pk.to_bytes()));
        xonly.push(hex(&xo.serialize()));
    }
    KeySet {
        labels: LABELS[..count].iter().map(|s| s.to_string()).collect(),
        compressed,
        xonly,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_valid_points() {
        let mut rng = SeededRng::new(1);
        let ks = generate(&mut rng, 3);
        assert_eq!(ks.compressed.len(), 3);
        for hex in &ks.compressed {
            assert_eq!(hex.len(), 66);
            assert!(hex.starts_with("02") || hex.starts_with("03"));
            let bytes: Vec<u8> = (0..33)
                .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
                .collect();
            assert!(bitcoin::key::PublicKey::from_slice(&bytes).is_ok());
        }
        for hex in &ks.xonly {
            assert_eq!(hex.len(), 64);
            let bytes: Vec<u8> = (0..32)
                .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
                .collect();
            assert!(XOnlyPublicKey::from_slice(&bytes).is_ok());
        }
    }
}
