//! 64-bit floating point by integer operations: the host (D3DMetal, Metal) has no fp64, and the
//! console's shaders use a little of it (`v_cvt_f32_f64` of a constant stored as a double,
//! `v_add_f64` sums). The algorithms are written once over [`Ops`], on values that are 64-bit
//! integers, and run on the IR builder in the lowering and on `u64` in the tests, where they are
//! checked against the CPU's own `f64`.
//!
//! Round-to-nearest-even throughout, denormals included (the f32 result of a conversion may be
//! one too).

/// What the algorithms need: 64-bit integer values and conditions. Shifts are by amounts below 64.
pub(crate) trait Ops {
    type V: Copy;
    type C: Copy;
    fn k(&mut self, v: u64) -> Self::V;
    fn and(&mut self, a: Self::V, b: Self::V) -> Self::V;
    fn or(&mut self, a: Self::V, b: Self::V) -> Self::V;
    #[allow(dead_code)]
    fn xor(&mut self, a: Self::V, b: Self::V) -> Self::V;
    fn add(&mut self, a: Self::V, b: Self::V) -> Self::V;
    fn sub(&mut self, a: Self::V, b: Self::V) -> Self::V;
    fn shl(&mut self, a: Self::V, by: Self::V) -> Self::V;
    fn lshr(&mut self, a: Self::V, by: Self::V) -> Self::V;
    fn eq(&mut self, a: Self::V, b: Self::V) -> Self::C;
    fn ne(&mut self, a: Self::V, b: Self::V) -> Self::C;
    /// Unsigned a < b.
    fn ult(&mut self, a: Self::V, b: Self::V) -> Self::C;
    /// Signed a >= b.
    fn sge(&mut self, a: Self::V, b: Self::V) -> Self::C;
    fn sel(&mut self, c: Self::C, a: Self::V, b: Self::V) -> Self::V;
}

fn kshl<O: Ops>(o: &mut O, a: O::V, n: u64) -> O::V {
    let n = o.k(n);
    o.shl(a, n)
}

fn klshr<O: Ops>(o: &mut O, a: O::V, n: u64) -> O::V {
    let n = o.k(n);
    o.lshr(a, n)
}

fn kn_plus_one<O: Ops>(o: &mut O, n: u64) -> O::V {
    o.k(n + 1)
}

fn kand<O: Ops>(o: &mut O, a: O::V, m: u64) -> O::V {
    let m = o.k(m);
    o.and(a, m)
}

/// `v_cvt_f32_f64`: the f32 bits (in the low 32 bits of the result) of the double `b`.
pub(crate) fn cvt_f32_f64<O: Ops>(o: &mut O, b: O::V) -> O::V {
    let sign = klshr(o, b, 63);
    let e = klshr(o, b, 52);
    let e = kand(o, e, 0x7ff);
    let m = kand(o, b, (1 << 52) - 1);
    let (zero, one) = (o.k(0), o.k(1));
    // infinity and NaN (a quiet NaN with the payload's top bits)
    let payload = klshr(o, m, 29);
    let qnan = o.k(0x40_0000);
    let payload = o.or(payload, qnan);
    let m_zero = o.eq(m, zero);
    let payload = o.sel(m_zero, zero, payload);
    let inf = o.k(0x7f80_0000);
    let special = o.or(inf, payload);
    // normal results
    let bias = o.k(896); // 1023 - 127
    let ee = o.sub(e, bias);
    let rem = kand(o, m, 0x1fff_ffff);
    let q = klshr(o, m, 29);
    let half = o.k(1 << 28);
    let above = o.ult(half, rem);
    let tie = o.eq(rem, half);
    let odd = o.and(q, one);
    let up_tie = o.sel(tie, odd, zero);
    let up = o.sel(above, one, up_tie);
    let ee23 = kshl(o, ee, 23);
    let normal = o.add(ee23, q);
    let normal = o.add(normal, up);
    // results below the smallest normal
    let implicit = o.k(1 << 52);
    let mant = o.or(m, implicit);
    let thirty = o.k(30);
    let raw = o.sub(thirty, ee);
    let sixty3 = o.k(63);
    let too_far = o.ult(sixty3, raw); // raw > 63, or negative (huge)
    let raw = o.sel(too_far, sixty3, raw);
    let ee_pos = o.sge(ee, one);
    let s = o.sel(ee_pos, thirty, raw);
    let q2 = o.lshr(mant, s);
    let mask = o.shl(one, s);
    let mask = o.sub(mask, one);
    let rem2 = o.and(mant, mask);
    let sm1 = o.sub(s, one);
    let half2 = o.shl(one, sm1);
    let above2 = o.ult(half2, rem2);
    let tie2 = o.eq(rem2, half2);
    let odd2 = o.and(q2, one);
    let up_tie2 = o.sel(tie2, odd2, zero);
    let up2 = o.sel(above2, one, up_tie2);
    let denormal = o.add(q2, up2);
    // choose
    let k255 = o.k(255);
    let big = o.sge(ee, k255);
    let mag = o.sel(ee_pos, normal, denormal);
    let mag = o.sel(big, inf, mag);
    let e_zero = o.eq(e, zero);
    let mag = o.sel(e_zero, zero, mag);
    let k7ff = o.k(0x7ff);
    let e_all = o.eq(e, k7ff);
    let mag = o.sel(e_all, special, mag);
    let sign = kshl(o, sign, 31);
    o.or(sign, mag)
}

/// `v_add_f64`: the bits of `a + b` for the doubles `a` and `b`.
pub(crate) fn add_f64<O: Ops>(o: &mut O, a: O::V, b: O::V) -> O::V {
    let (zero, one) = (o.k(0), o.k(1));
    let magnitude = |o: &mut O, x: O::V| kand(o, x, (1 << 63) - 1);
    let (ma, mb) = (magnitude(o, a), magnitude(o, b));
    // `big` has the larger magnitude
    let swap = o.ult(ma, mb);
    let big = o.sel(swap, b, a);
    let small = o.sel(swap, a, b);
    let parts = |o: &mut O, x: O::V| {
        let s = klshr(o, x, 63);
        let e = klshr(o, x, 52);
        let e = kand(o, e, 0x7ff);
        let m = kand(o, x, (1 << 52) - 1);
        (s, e, m)
    };
    let (sb, eb0, mb_) = parts(o, big);
    let (ss, es0, ms_) = parts(o, small);
    let k7ff = o.k(0x7ff);
    // 53-bit significands (a denormal has no implicit bit and the exponent 1)
    let implicit = o.k(1 << 52);
    let sig = |o: &mut O, e: O::V, m: O::V| {
        let full = o.or(m, implicit);
        let z = o.k(0);
        let is0 = o.eq(e, z);
        o.sel(is0, m, full)
    };
    let (mant_b, mant_s) = (sig(o, eb0, mb_), sig(o, es0, ms_));
    let eff = |o: &mut O, e: O::V| {
        let z = o.k(0);
        let one = o.k(1);
        let is0 = o.eq(e, z);
        o.sel(is0, one, e)
    };
    let (eb, es) = (eff(o, eb0), eff(o, es0));
    // three extra low bits (guard, round, sticky)
    let mant_b = kshl(o, mant_b, 3);
    let mant_s = kshl(o, mant_s, 3);
    // align the smaller one
    let d = o.sub(eb, es);
    let sixty3 = o.k(63);
    let far = o.ult(sixty3, d);
    let sh = o.sel(far, sixty3, d);
    let shifted = o.lshr(mant_s, sh);
    let mask = o.shl(one, sh);
    let mask = o.sub(mask, one);
    let lost = o.and(mant_s, mask);
    let lost = o.ne(lost, zero);
    let sticky = o.sel(lost, one, zero);
    let shifted = o.or(shifted, sticky);
    // add or subtract
    let same = o.eq(sb, ss);
    let sum = o.add(mant_b, shifted);
    let diff = o.sub(mant_b, shifted);
    let mut v = o.sel(same, sum, diff);
    let mut e = eb;
    // carry out: bit 56
    let top = o.k(1 << 56);
    let carry = o.and(v, top);
    let carried = o.ne(carry, zero);
    let lowbit = o.and(v, one);
    let halved = klshr(o, v, 1);
    let halved = o.or(halved, lowbit);
    v = o.sel(carried, halved, v);
    let e_plus = o.add(e, one);
    e = o.sel(carried, e_plus, e);
    // cancellation: shift left until bit 55 is set
    for n in [32u64, 16, 8, 4, 2, 1] {
        // the top n bits below bit 56: bits 55 down to 56 - n
        let ones = (1u64 << n) - 1;
        let field = ones << (56 - n);
        let top_bits = kand(o, v, field);
        let empty = o.eq(top_bits, zero);
        let nonzero = o.ne(v, zero);
        let kn = o.k(n);
        let shift = o.sel(empty, kn, zero);
        let shift = o.sel(nonzero, shift, zero);
        // not below the exponent 1: the result is then a denormal
        let np1 = kn_plus_one(o, n);
        let allowed = o.sge(e, np1);
        let shift = o.sel(allowed, shift, zero);
        v = o.shl(v, shift);
        e = o.sub(e, shift);
    }
    // round to nearest even
    let low = kand(o, v, 7);
    let m = klshr(o, v, 3);
    let k4 = o.k(4);
    let above = o.ult(k4, low);
    let tie = o.eq(low, k4);
    let odd = o.and(m, one);
    let up_tie = o.sel(tie, odd, zero);
    let up = o.sel(above, one, up_tie);
    let m = o.add(m, up);
    // (exponent - 1) << 52 plus the significand with its hidden bit: a carry out of the
    // significand, and a denormal that rounds up to the smallest normal, add to the exponent
    let e_minus = o.sub(e, one);
    let e_field = kshl(o, e_minus, 52);
    let bits = o.add(e_field, m);
    let sign = kshl(o, sb, 63);
    let inf_mag = o.k(0x7ff0_0000_0000_0000);
    let overflow = o.ult(bits, inf_mag);
    let mag = o.sel(overflow, bits, inf_mag);
    let finite = o.or(sign, mag);
    // exact cancellation: +0, or -0 for two negative zeros
    let result_zero = o.eq(v, zero);
    let neg_zero = o.sel(same, sign, zero);
    let r = o.sel(result_zero, neg_zero, finite);
    // infinities and NaNs
    let a_all = {
        let ea = {
            let t = klshr(o, a, 52);
            kand(o, t, 0x7ff)
        };
        o.eq(ea, k7ff)
    };
    let b_all = {
        let eb2 = {
            let t = klshr(o, b, 52);
            kand(o, t, 0x7ff)
        };
        o.eq(eb2, k7ff)
    };
    let a_m = kand(o, a, (1 << 52) - 1);
    let b_m = kand(o, b, (1 << 52) - 1);
    let a_nan = {
        let nz = o.ne(a_m, zero);
        let x = o.sel(nz, one, zero);
        o.sel(a_all, x, zero)
    };
    let b_nan = {
        let nz = o.ne(b_m, zero);
        let x = o.sel(nz, one, zero);
        o.sel(b_all, x, zero)
    };
    let nan_bits = o.k(0x7ff8_0000_0000_0000);
    let any_nan = o.or(a_nan, b_nan);
    let any_nan = o.ne(any_nan, zero);
    // a (or b) is an infinity: the result is that infinity, a NaN if the other is the opposite one
    let a_inf = a_all;
    let b_inf = b_all;
    let (sa, sb2) = (klshr(o, a, 63), klshr(o, b, 63));
    let signs_differ = o.ne(sa, sb2);
    let both_inf = {
        let x = o.sel(a_inf, one, zero);
        let y = o.sel(b_inf, one, zero);
        let z = o.and(x, y);
        o.ne(z, zero)
    };
    let inf_nan = {
        let x = o.sel(both_inf, one, zero);
        let y = o.sel(signs_differ, one, zero);
        let z = o.and(x, y);
        o.ne(z, zero)
    };
    let r = o.sel(b_inf, b, r);
    let r = o.sel(a_inf, a, r);
    let nan_case = {
        let x = o.sel(any_nan, one, zero);
        let y = o.sel(inf_nan, one, zero);
        let z = o.or(x, y);
        o.ne(z, zero)
    };
    o.sel(nan_case, nan_bits, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The algorithms on `u64`.
    struct Native;

    impl Ops for Native {
        type V = u64;
        type C = bool;
        fn k(&mut self, v: u64) -> u64 { v }
        fn and(&mut self, a: u64, b: u64) -> u64 { a & b }
        fn or(&mut self, a: u64, b: u64) -> u64 { a | b }
        fn xor(&mut self, a: u64, b: u64) -> u64 { a ^ b }
        fn add(&mut self, a: u64, b: u64) -> u64 { a.wrapping_add(b) }
        fn sub(&mut self, a: u64, b: u64) -> u64 { a.wrapping_sub(b) }
        fn shl(&mut self, a: u64, by: u64) -> u64 { assert!(by < 64, "shift by {by}"); a << by }
        fn lshr(&mut self, a: u64, by: u64) -> u64 { assert!(by < 64, "shift by {by}"); a >> by }
        fn eq(&mut self, a: u64, b: u64) -> bool { a == b }
        fn ne(&mut self, a: u64, b: u64) -> bool { a != b }
        fn ult(&mut self, a: u64, b: u64) -> bool { a < b }
        fn sge(&mut self, a: u64, b: u64) -> bool { a as i64 >= b as i64 }
        fn sel(&mut self, c: bool, a: u64, b: u64) -> u64 { if c { a } else { b } }
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    fn cvt(x: f64) -> u32 {
        cvt_f32_f64(&mut Native, x.to_bits()) as u32
    }

    fn same_f32(got: u32, want: f32) -> bool {
        if want.is_nan() {
            f32::from_bits(got).is_nan()
        } else {
            got == want.to_bits()
        }
    }

    #[test]
    #[allow(clippy::approx_constant)] // 3.14159265358979 is a test value, not the constant
    fn f32_from_f64_edges() {
        let cases = [
            0.0, -0.0, 1.0, -1.0, 0.5, 3.14159265358979, 1e-10, 1e10, 1e38, 3.4028234663852886e38, 3.4028235677973366e38,
            3.4028236e38, 1e39, -1e39, f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 1e-38, 1.1754943508222875e-38, 1e-40,
            1.4e-45, 7e-46, 7.006492321624085e-46, 7.006492321624086e-46, 1e-50, 5e-324, 2.2250738585072014e-308,
            // ties: 1 + 2^-24 (halfway between two f32), and just above and below
            1.0 + 2f64.powi(-24), 1.0 + 2f64.powi(-24) + 2f64.powi(-40), 1.0 + 3.0 * 2f64.powi(-24),
            16777217.0, 16777219.0, 8388609.5,
        ];
        for x in cases {
            let got = cvt(x);
            assert!(same_f32(got, x as f32), "{x:e}: got {got:#010x}, want {:#010x}", (x as f32).to_bits());
        }
    }

    #[test]
    fn f32_from_f64_random() {
        let mut r = Rng(0x1234_5678_9abc_def1);
        for i in 0..400_000u32 {
            let bits = r.next();
            // spread over exponents near the f32 range as well as everywhere
            let bits = if i % 2 == 0 { bits } else { (bits & !(0x7ff << 52)) | ((896 - 30 + (r.next() % 330)) << 52) };
            let x = f64::from_bits(bits);
            let got = cvt(x);
            assert!(same_f32(got, x as f32), "{bits:#018x} {x:e}: got {got:#010x}, want {:#010x}", (x as f32).to_bits());
        }
    }

    fn add(a: f64, b: f64) -> u64 {
        add_f64(&mut Native, a.to_bits(), b.to_bits())
    }

    fn same_f64(got: u64, want: f64) -> bool {
        let g = f64::from_bits(got);
        if want.is_nan() {
            g.is_nan()
        } else {
            got == want.to_bits()
        }
    }

    #[test]
    fn f64_sums_edges() {
        let vals = [
            0.0, -0.0, 1.0, -1.0, 0.5, 3.0, -3.0, 1e300, -1e300, 1.7976931348623157e308, 2.0f64.powi(-1022), 1.5,
            5e-324, -5e-324, 2.2250738585072014e-308, 1.0 + f64::EPSILON, 1.0 - f64::EPSILON / 2.0, 1e-300, f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 123456.789, 2f64.powi(53),
            2f64.powi(53) + 2.0, 4.0 / 3.0,
        ];
        for a in vals {
            for b in vals {
                let got = add(a, b);
                assert!(same_f64(got, a + b), "{a:e} + {b:e}: got {:e} ({got:#x}), want {:e} ({:#x})", f64::from_bits(got), a + b, (a + b).to_bits());
            }
        }
        // signed zeros
        assert_eq!(add(-0.0, -0.0), (-0.0f64).to_bits());
        assert_eq!(add(0.0, -0.0), 0.0f64.to_bits());
        assert_eq!(add(1.0, -1.0), 0.0f64.to_bits());
    }

    #[test]
    fn f64_sums_random() {
        let mut r = Rng(0x9e37_79b9_7f4a_7c15);
        for i in 0..400_000u32 {
            let (mut x, mut y) = (r.next(), r.next());
            // exponents within a few bits of each other make cancellation and carries common
            if i % 3 != 0 {
                let e = 1000 + (r.next() % 50);
                x = (x & !(0x7ff << 52)) | (e << 52);
                y = (y & !(0x7ff << 52)) | ((e + (r.next() % 4) - 1) << 52);
            }
            if i % 7 == 0 {
                // a nearly cancelling pair
                y = x ^ (1 << 63) ^ (r.next() & 0xfff);
            }
            let (a, b) = (f64::from_bits(x), f64::from_bits(y));
            let got = add(a, b);
            assert!(same_f64(got, a + b), "{x:#018x} + {y:#018x}: got {got:#018x}, want {:#018x}", (a + b).to_bits());
        }
    }
}
