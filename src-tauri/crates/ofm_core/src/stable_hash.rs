//! A hash whose answer is allowed to be written into a save file.
//!
//! Several AI policies need to derive a decision from an id rather than roll for
//! it — which deputy a manager misjudges today, which weekday a club sits down
//! to review how it plays. Those answers end up in a saved season, so the hash
//! behind them has to give the same number next year and on the next toolchain.
//! `DefaultHasher` does not promise that, which is why this exists rather than a
//! `use std::hash`.

/// FNV-1a, hand-rolled.
///
/// `seed` is folded into the offset basis, so nesting calls — hashing a player
/// id under a club-and-date seed, say — keeps the streams independent.
pub(crate) fn stable_hash(bytes: &[u8], seed: u64) -> u64 {
    let mut hasher = StableHasher::new(seed);
    hasher.write(bytes);
    hasher.finish()
}

/// [`stable_hash`] fed in pieces, so a caller with fields to hash needs no buffer to join them in.
pub(crate) struct StableHasher {
    hash: u64,
}

impl StableHasher {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            hash: 0xcbf2_9ce4_8422_2325 ^ seed,
        }
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    pub(crate) fn finish(self) -> u64 {
        self.hash
    }
}

/// A hash as a number from 0 up to but not including 1: the top 53 bits, so every value is exact in
/// an `f64`. The one way the game turns a hash into a draw.
pub(crate) fn unit_interval(hash: u64) -> f64 {
    (hash >> 11) as f64 / (1u64 << 53) as f64
}

/// MurmurHash3's 64-bit finaliser. [`stable_hash`] folds its seed into the start of the hash, so two
/// seeds over the same input give outputs that move together (measured at r = 0.96 for two salts
/// over one manager id); mixing the output makes each seed's draw independent of the others'. Use
/// it whenever one input is drawn from more than once under different seeds.
pub(crate) fn mixed(mut hash: u64) -> u64 {
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    hash ^ (hash >> 33)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_bytes_hash_the_same_way_every_time() {
        assert_eq!(stable_hash(b"club-42", 0), stable_hash(b"club-42", 0));
    }

    #[test]
    fn writing_bytes_in_pieces_hashes_them_as_one_slice() {
        let bytes = b"club-42-members-17";
        let mut hasher = StableHasher::new(9);
        hasher.write(&bytes[..4]);
        hasher.write(&bytes[4..9]);
        hasher.write(&bytes[9..]);
        assert_eq!(hasher.finish(), stable_hash(bytes, 9));
    }

    #[test]
    fn the_seed_changes_the_stream() {
        assert_ne!(stable_hash(b"club-42", 0), stable_hash(b"club-42", 1));
    }

    /// The value itself is load-bearing: a saved season derives AI decisions
    /// from it, so changing the constants would silently re-roll every one of
    /// them. Pinned rather than described.
    #[test]
    fn the_answer_is_pinned_to_a_number_a_save_file_can_rely_on() {
        assert_eq!(stable_hash(b"club-42", 0), 17_696_388_274_473_556_074);
    }

    /// Given the smallest and largest hashes
    /// When each is turned into a draw
    /// Then both lie in [0, 1) and are exact, so a draw can never be 1.
    #[test]
    fn a_draw_is_always_below_one() {
        assert_eq!(unit_interval(0), 0.0);
        assert!(unit_interval(u64::MAX) < 1.0);
    }

    /// Given one input hashed under two seeds
    /// When the two hashes are mixed
    /// Then the outputs still differ and the mixing is itself deterministic.
    #[test]
    fn mixing_keeps_seeds_apart_and_is_deterministic() {
        let (a, b) = (stable_hash(b"manager", 1), stable_hash(b"manager", 2));
        assert_ne!(mixed(a), mixed(b));
        assert_eq!(mixed(a), mixed(a));
    }
}
