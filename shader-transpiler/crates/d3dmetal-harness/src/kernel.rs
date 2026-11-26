//! Host-side specification of the synthetic SM 6.6 compute kernel.
//!
//! `probe/shaders/fill_cs66.hlsl` is the GPU implementation; this module is
//! the independent host computation the readback is judged against. The probe
//! only reports what the GPU wrote.

/// Thread groups dispatched by the probe.
pub const GROUPS: u32 = 4;
/// `[numthreads(64, 1, 1)]` in `fill_cs66.hlsl`.
pub const THREADS_PER_GROUP: u32 = 64;
/// One 32-bit word per thread.
pub const WORDS: usize = (GROUPS * THREADS_PER_GROUP) as usize;
/// Value the probe stores in every word before the dispatch.
pub const SENTINEL: u32 = 0xA5A5_A5A5;

/// The word thread `index` must write.
pub fn expected_word(index: u32) -> u32 {
    let mut v = index.wrapping_mul(0x9E37_79B1);
    v ^= v >> 15;
    v.wrapping_add(0x0123_4567)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference values computed separately (Python, 32-bit wrapping arithmetic).
    #[test]
    fn matches_independently_computed_values() {
        let reference = [
            (0, 0x0123_4567),
            (1, 0x9f59_8b46),
            (2, 0x3d91_d126),
            (63, 0xf0ca_7329),
            (64, 0x8f02_bd63),
            (255, 0x9a66_4b32),
        ];
        for (index, want) in reference {
            assert_eq!(expected_word(index), want, "index {index}");
        }
    }

    #[test]
    fn no_expected_word_equals_the_sentinel() {
        assert!((0..WORDS as u32).all(|i| expected_word(i) != SENTINEL));
    }
}
