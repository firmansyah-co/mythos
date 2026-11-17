//! The real functions of IEC 61131-3 ed. 3 Table 28 (SQRT, LN, LOG, EXP, SIN, COS, TAN, ASIN,
//! ACOS, ATAN, ATAN2) and EXPT (Table 29), without `std`.
//!
//! `core` has no floating-point functions on the targets the VM runs on (no libm), and the VM
//! takes no dependency (ADR-013 §2.1). So they are here, in `f64`; a REAL's is the `f64`
//! result rounded to `f32`. The methods are the classic ones of fdlibm (Sun Microsystems,
//! 1993: "Permission to use, copy, modify, and distribute this software is freely granted,
//! provided that this notice is preserved"), whose polynomial coefficients are used: argument
//! reduction to a small interval, a minimax polynomial there, and the reduction undone. The
//! code is written here, not copied. SQRT is exact (correctly rounded, as IEC 60559 requires).
//! The accuracy of the others, which the standard leaves to the implementer (§6.6.2.5.8),
//! is measured against the host's libm by `math_against_the_host.rs` and stated in ADR-013.
//!
//! IEC 60559 values are given where a result is not a real number (SQRT(-1) and LN(-1) are
//! NaN, LN(0) is -infinity, EXP(1000) is +infinity), as for REAL division.

// The coefficients are written as fdlibm publishes them, digit for digit, so they can be
// checked against it; the compiler rounds each to the nearest double, which is the value
// fdlibm uses. Where a constant is one of `core`'s, `core`'s is used.
#![allow(clippy::excessive_precision)]

use core::f64::consts::{FRAC_2_PI, FRAC_PI_2, FRAC_PI_4, LOG2_E};

const LN2_HI: f64 = 6.931_471_803_691_238_164_90e-01;
const LN2_LO: f64 = 1.908_214_929_270_587_700_02e-10;
const INV_LN2: f64 = LOG2_E;
const PI: f64 = core::f64::consts::PI;
const PI_LO: f64 = 1.224_646_799_147_353_207_2e-16;
const PIO2_HI: f64 = FRAC_PI_2;
const PIO2_LO: f64 = 6.123_233_995_736_766_035_87e-17;

fn bits(x: f64) -> u64 {
    x.to_bits()
}
fn abs(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & !(1 << 63))
}
fn is_nan(x: f64) -> bool {
    x.is_nan()
}
/// 2^k as an f64, for k in -1022..=1023.
fn pow2(k: i32) -> f64 {
    f64::from_bits(((k + 1023) as u64) << 52)
}
/// x · 2^k for any k, through two steps where one would leave the range.
fn scale(x: f64, k: i32) -> f64 {
    if k > 1023 {
        x * pow2(1023) * pow2(k - 1023)
    } else if k < -1022 {
        // The first step is exact; the second rounds once into the subnormals.
        x * pow2(k + 1000) * pow2(-1000)
    } else {
        x * pow2(k)
    }
}
/// Round to the nearest integer, a half away from zero; |x| < 2^52.
fn round(x: f64) -> f64 {
    let t = x as i64 as f64;
    let d = x - t;
    if d >= 0.5 {
        t + 1.0
    } else if d <= -0.5 {
        t - 1.0
    } else {
        t
    }
}

/// The square root, correctly rounded: the 53 bits of the result and one more are found
/// bit by bit, the rest only for whether it is exact.
pub fn sqrt(x: f64) -> f64 {
    if is_nan(x) || x == 0.0 || x == f64::INFINITY {
        return x;
    }
    if x < 0.0 {
        return f64::NAN;
    }
    // x = m · 2^e, m an integer with its top bit at 52 (subnormals normalised).
    let b = bits(x);
    let mut e = ((b >> 52) & 0x7FF) as i32;
    let mut m = b & ((1 << 52) - 1);
    if e == 0 {
        while m & (1 << 52) == 0 {
            m <<= 1;
            e -= 1;
        }
        e += 1;
    } else {
        m |= 1 << 52;
    }
    let mut e = e - 1075;
    if e & 1 != 0 {
        m <<= 1;
        e -= 1;
    }
    // sqrt(m · 2^e) = sqrt(m · 2^64) · 2^(e/2 - 32); m · 2^64 has 117 or 118 bits, its root 59.
    let n: u128 = (m as u128) << 64;
    let (mut root, mut rem): (u128, u128) = (0, 0);
    for k in (0..64).rev() {
        rem = (rem << 2) | ((n >> (2 * k)) & 3);
        let trial = (root << 2) | 1;
        root <<= 1;
        if rem >= trial {
            rem -= trial;
            root |= 1;
        }
    }
    let exact = rem == 0;
    let r = root as u64; // 58 or 59 bits
    let width = 64 - r.leading_zeros() as i32;
    let drop = width - 53;
    let mut mant = r >> drop;
    let dropped = r & ((1u64 << drop) - 1);
    let half = 1u64 << (drop - 1);
    if dropped > half || (dropped == half && (!exact || mant & 1 == 1)) {
        mant += 1;
    }
    let mut exp = e / 2 - 32 + drop;
    if mant == 1 << 53 {
        mant >>= 1;
        exp += 1;
    }
    // mant · 2^exp, mant with its top bit at 52: always a normal number.
    f64::from_bits((((exp + 52 + 1023) as u64) << 52) | (mant & ((1 << 52) - 1)))
}

/// e^x.
pub fn exp(x: f64) -> f64 {
    const P1: f64 = 1.666_666_666_666_660_190_37e-01;
    const P2: f64 = -2.777_777_777_701_559_338_42e-03;
    const P3: f64 = 6.613_756_321_437_934_361_17e-05;
    const P4: f64 = -1.653_390_220_546_525_153_90e-06;
    const P5: f64 = 4.138_136_797_057_238_460_39e-08;
    if is_nan(x) {
        return x;
    }
    if x > 709.782_712_893_383_973_096 {
        return f64::INFINITY;
    }
    if x < -745.133_219_101_941_108_420 {
        return 0.0;
    }
    if abs(x) < 3.725_290_298_461_914e-9 {
        return 1.0 + x;
    }
    // x = k·ln2 + r, |r| <= ln2/2, r = hi - lo exactly enough.
    let k = round(x * INV_LN2);
    let hi = x - k * LN2_HI;
    let lo = k * LN2_LO;
    let r = hi - lo;
    let t = r * r;
    let c = r - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    let y = 1.0 - ((lo - (r * c) / (2.0 - c)) - hi);
    scale(y, k as i32)
}

/// ln x, from x = 2^k (1 + f) with 1 + f in [√2/2, √2): the reduced part as log of
/// (1 + s)/(1 - s), s = f / (2 + f), whose series is in s².
fn log_parts(x: f64) -> (f64, f64) {
    const LG1: f64 = 6.666_666_666_666_735_130e-01;
    const LG2: f64 = 3.999_999_999_940_941_908e-01;
    const LG3: f64 = 2.857_142_874_366_239_149e-01;
    const LG4: f64 = 2.222_219_843_214_978_396e-01;
    const LG5: f64 = 1.818_357_216_161_805_012e-01;
    const LG6: f64 = 1.531_383_769_920_937_332e-01;
    const LG7: f64 = 1.479_819_860_511_658_591e-01;
    let mut b = bits(x);
    let mut k: i32 = 0;
    if b >> 52 == 0 {
        // A subnormal: made normal first.
        b = bits(x * pow2(54));
        k -= 54;
    }
    k += ((b >> 52) & 0x7FF) as i32 - 1023;
    // The mantissa with exponent 0, then halved when it is past √2.
    let mut m = f64::from_bits((b & ((1 << 52) - 1)) | (1023 << 52));
    if m > core::f64::consts::SQRT_2 {
        m *= 0.5;
        k += 1;
    }
    let f = m - 1.0;
    let s = f / (2.0 + f);
    let z = s * s;
    let w = z * z;
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    let r = t2 + t1;
    let hfsq = 0.5 * f * f;
    let k = k as f64;
    // ln x = k ln2 + f - hfsq + s (hfsq + R), the small terms summed first. k·LN2_HI is
    // exact (LN2_HI ends in zeros); its sum with f is kept with its rounding error.
    let (hi, err) = two_sum(k * LN2_HI, f);
    let lo = err + (k * LN2_LO + (s * (hfsq + r) - hfsq));
    // As ln x rounded, and what that rounding left.
    two_sum(hi, lo)
}

/// a + b exactly, as a sum of two doubles (Knuth).
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let v = s - a;
    (s, (a - (s - v)) + (b - v))
}

/// ln x.
pub fn ln(x: f64) -> f64 {
    if is_nan(x) || x == f64::INFINITY {
        return x;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x < 0.0 {
        return f64::NAN;
    }
    let (hi, lo) = log_parts(x);
    hi + lo
}

/// log10 x: ln x / ln 10, with ln x carried in two parts so the division costs no more than
/// one rounding of the product.
pub fn log10(x: f64) -> f64 {
    // 1/ln 10 as hi + lo, hi with its last 32 bits zero so that a product with it is
    // exact for a factor of 21 bits (computed for this file to 80 digits).
    const INV_LN10_HI: f64 = 0.434_294_462_203_979_5;
    const INV_LN10_LO: f64 = 1.969_927_233_546_362_7e-8;
    if is_nan(x) || x == f64::INFINITY {
        return x;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x < 0.0 {
        return f64::NAN;
    }
    let (hi, lo) = log_parts(x);
    // ln x = h + l, h with its last 32 bits zero: h · INV_LN10_HI is exact.
    let h = f64::from_bits(bits(hi) & 0xFFFF_FFFF_0000_0000);
    let l = (hi - h) + lo;
    h * INV_LN10_HI + (l * INV_LN10_HI + (h + l) * INV_LN10_LO)
}

/// x mod π/2: (n mod 4, y0 + y1) with x = n·π/2 + y0 + y1, |y0| <= π/4 about.
fn rem_pio2(x: f64) -> (i32, f64, f64) {
    const INV_PIO2: f64 = FRAC_2_PI;
    const PIO2_1: f64 = 1.570_796_326_734_125_614_17e+00;
    const PIO2_1T: f64 = 6.077_100_506_506_192_249_32e-11;
    const PIO2_2: f64 = 6.077_100_506_303_965_976_60e-11;
    const PIO2_2T: f64 = 2.022_266_248_795_950_631_54e-21;
    const PIO2_3: f64 = 2.022_266_248_711_166_455_80e-21;
    const PIO2_3T: f64 = 8.478_427_660_368_899_569_97e-32;
    let ax = abs(x);
    if ax <= FRAC_PI_4 {
        return (0, x, 0.0);
    }
    if ax < 1_647_099.0 {
        // Up to about 2^20 · π/2: n·π/2 in three parts of 33 bits, each product exact.
        let n = round(x * INV_PIO2);
        let ex = ((bits(x) >> 52) & 0x7FF) as i32;
        let mut r = x - n * PIO2_1;
        let mut w = n * PIO2_1T;
        let mut y0 = r - w;
        let lost = |y: f64| ex - ((bits(y) >> 52) & 0x7FF) as i32;
        if lost(y0) > 16 {
            let t = r;
            w = n * PIO2_2;
            r = t - w;
            w = n * PIO2_2T - ((t - r) - w);
            y0 = r - w;
            if lost(y0) > 49 {
                let t = r;
                w = n * PIO2_3;
                r = t - w;
                w = n * PIO2_3T - ((t - r) - w);
                y0 = r - w;
            }
        }
        let y1 = (r - y0) - w;
        return (((n as i64) & 3) as i32, y0, y1);
    }
    if is_nan(x) || ax == f64::INFINITY {
        return (0, f64::NAN, 0.0);
    }
    large_rem_pio2(x)
}

/// 2/π, its first 1,280 bits after the point, 32 to a word: computed for this file from π
/// by Machin's formula (`floor(2^1280 · 2/π)`).
const TWO_OVER_PI: [u32; 40] = [
    0xA2F9836E, 0x4E441529, 0xFC2757D1, 0xF534DDC0, 0xDB629599, 0x3C439041, 0xFE5163AB, 0xDEBBC561,
    0xB7246E3A, 0x424DD2E0, 0x06492EEA, 0x09D1921C, 0xFE1DEB1C, 0xB129A73E, 0xE88235F5, 0x2EBB4484,
    0xE99C7026, 0xB45F7E41, 0x3991D639, 0x835339F4, 0x9C845F8B, 0xBDF9283B, 0x1FF897FF, 0xDE05980F,
    0xEF2F118B, 0x5A0A6D1F, 0x6D367ECF, 0x27CB09B7, 0x4F463F66, 0x9E5FEA2D, 0x7527BAC7, 0xEBE5F17B,
    0x3D0739F7, 0x8A5292EA, 0x6BFB5FB1, 0x1F8D5D08, 0x56033046, 0xFC7B6BAB, 0xF0CFBC20, 0x9AF4361D,
];

/// Bit `i` of 2/π after the point, counting the first as 1; 0 past the table.
fn two_over_pi_bit(i: i32) -> u64 {
    if !(1..=1280).contains(&i) {
        return 0;
    }
    let i = (i - 1) as usize;
    ((TWO_OVER_PI[i / 32] >> (31 - i % 32)) & 1) as u64
}

/// The reduction for |x| >= 2^20 · π/2 (Payne and Hanek): x = m · 2^E, m an integer of 53
/// bits; of x · 2/π only the bits from 2/π's bit E-1 on matter (the ones before give a
/// multiple of 4), and 192 of them give the fraction to 2^-139.
fn large_rem_pio2(x: f64) -> (i32, f64, f64) {
    let b = bits(x);
    // |x| = m · 2^e, m an integer of 53 bits.
    let e = ((b >> 52) & 0x7FF) as i32 - 1075;
    let m = (b & ((1 << 52) - 1)) | (1 << 52);
    // W: bits e-1 .. e+190 of 2/π as an integer of 192 bits (three words, the highest
    // first). Then |x| · 2/π = m · W · 2^-190 plus a multiple of 4 (the bits before e-1)
    // plus less than 2^-137 (the bits after e+190).
    let mut w = [0u64; 3];
    for (j, word) in w.iter_mut().enumerate() {
        for k in 0..64 {
            *word = (*word << 1) | two_over_pi_bit(e - 1 + (j * 64 + k) as i32);
        }
    }
    // m · W in four words, the highest first.
    let mut prod = [0u64; 4];
    let mut carry: u128 = 0;
    for j in (0..3).rev() {
        let t = (m as u128) * (w[j] as u128) + carry;
        prod[j + 1] = t as u64;
        carry = t >> 64;
    }
    prod[0] = carry as u64;
    // The point is 190 bits from the bottom: the two bits above it are n mod 4, the 190
    // below the fraction, taken here as its first 192 bits.
    let mut n = (prod[1] >> 62) as i32;
    let mut f = [
        (prod[1] << 2) | (prod[2] >> 62),
        (prod[2] << 2) | (prod[3] >> 62),
        prod[3] << 2,
    ];
    // Past one half, the nearest multiple of π/2 is the next: the remainder is 1 - f below
    // zero.
    let below = f[0] >> 63 == 1;
    if below {
        n += 1;
        let mut borrow = false;
        for k in (0..3).rev() {
            let (d, b1) = 0u64.overflowing_sub(f[k]);
            let (d, b2) = d.overflowing_sub(borrow as u64);
            f[k] = d;
            borrow = b1 || b2;
        }
    }
    // The fraction as hi + lo: its first 53 bits exactly, then the next 75 or so.
    let hi = (f[0] >> 11) as f64 * pow2(-53);
    let rest = (((f[0] & 0x7FF) as u128) << 64) | f[1] as u128;
    let lo = rest as f64 * pow2(-128);
    // Times π/2, in two parts.
    let (p, err) = two_prod(hi, PIO2_HI);
    let q = err + hi * PIO2_LO + lo * PIO2_HI;
    let mut y0 = p + q;
    let mut y1 = q - (y0 - p);
    if below {
        y0 = -y0;
        y1 = -y1;
    }
    if x < 0.0 {
        return ((-n) & 3, -y0, -y1);
    }
    (n & 3, y0, y1)
}

/// a · b exactly, as a sum of two doubles (Dekker, with Veltkamp's split).
fn two_prod(a: f64, b: f64) -> (f64, f64) {
    const SPLIT: f64 = 134_217_729.0; // 2^27 + 1
    let p = a * b;
    let split = |v: f64| {
        let t = SPLIT * v;
        let h = t - (t - v);
        (h, v - h)
    };
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    let e = ((ah * bh - p) + ah * bl + al * bh) + al * bl;
    (p, e)
}

/// sin on [-π/4, π/4], of y0 + y1.
fn k_sin(x: f64, y: f64) -> f64 {
    const S1: f64 = -1.666_666_666_666_663_243_48e-01;
    const S2: f64 = 8.333_333_333_322_489_461_24e-03;
    const S3: f64 = -1.984_126_982_985_794_931_34e-04;
    const S4: f64 = 2.755_731_370_707_006_767_89e-06;
    const S5: f64 = -2.505_076_025_340_686_341_95e-08;
    const S6: f64 = 1.589_690_995_211_550_102_21e-10;
    let z = x * x;
    let w = z * z;
    let r = S2 + z * (S3 + z * S4) + z * w * (S5 + z * S6);
    let v = z * x;
    x - ((z * (0.5 * y - v * r) - y) - v * S1)
}

/// cos on [-π/4, π/4], of y0 + y1.
fn k_cos(x: f64, y: f64) -> f64 {
    const C1: f64 = 4.166_666_666_666_660_190_37e-02;
    const C2: f64 = -1.388_888_888_887_410_957_49e-03;
    const C3: f64 = 2.480_158_728_947_672_941_78e-05;
    const C4: f64 = -2.755_731_435_139_066_330_35e-07;
    const C5: f64 = 2.087_572_321_298_174_827_90e-09;
    const C6: f64 = -1.135_964_755_778_819_482_65e-11;
    let z = x * x;
    let w = z * z;
    let r = z * (C1 + z * (C2 + z * C3)) + w * w * (C4 + z * (C5 + z * C6));
    let hz = 0.5 * z;
    let one_minus = 1.0 - hz;
    one_minus + (((1.0 - one_minus) - hz) + (z * r - x * y))
}

pub fn sin(x: f64) -> f64 {
    let (n, y0, y1) = rem_pio2(x);
    match n {
        0 => k_sin(y0, y1),
        1 => k_cos(y0, y1),
        2 => -k_sin(y0, y1),
        _ => -k_cos(y0, y1),
    }
}

pub fn cos(x: f64) -> f64 {
    let (n, y0, y1) = rem_pio2(x);
    match n {
        0 => k_cos(y0, y1),
        1 => -k_sin(y0, y1),
        2 => -k_cos(y0, y1),
        _ => k_sin(y0, y1),
    }
}

pub fn tan(x: f64) -> f64 {
    let (n, y0, y1) = rem_pio2(x);
    let (s, c) = (k_sin(y0, y1), k_cos(y0, y1));
    if n & 1 == 0 {
        s / c
    } else {
        -c / s
    }
}

/// atan x: reduced to |x| < 7/16 through atan(x) = atan(c) + atan((x - c)/(1 + x c)) for
/// c in {0.5, 1, 1.5, ∞}, then a series in x².
pub fn atan(x: f64) -> f64 {
    const HI: [f64; 4] = [
        4.636_476_090_008_060_935_15e-01,
        FRAC_PI_4,
        9.827_937_232_473_290_540_82e-01,
        FRAC_PI_2,
    ];
    const LO: [f64; 4] = [
        2.269_877_745_296_168_709_24e-17,
        3.061_616_997_868_383_017_93e-17,
        1.390_331_103_123_099_845_16e-17,
        6.123_233_995_736_766_035_87e-17,
    ];
    const AT: [f64; 11] = [
        3.333_333_333_333_293_180_27e-01,
        -1.999_999_999_987_648_324_76e-01,
        1.428_571_427_250_346_637_11e-01,
        -1.111_111_040_546_235_578_80e-01,
        9.090_887_133_436_506_561_96e-02,
        -7.691_876_205_044_829_994_95e-02,
        6.661_073_137_387_531_206_69e-02,
        -5.833_570_133_790_573_486_45e-02,
        4.976_877_994_615_932_360_17e-02,
        -3.653_157_274_421_691_552_70e-02,
        1.628_582_011_536_578_236_23e-02,
    ];
    if is_nan(x) {
        return x;
    }
    let ax = abs(x);
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    if ax >= 7.378_697_629_483_820_646_4e19 {
        // 2^66: atan is π/2 to the last bit.
        return sign * (HI[3] + LO[3]);
    }
    let (id, t): (i32, f64) = if ax < 0.4375 {
        if ax < 7.450_580_596_923_828e-9 {
            return x;
        }
        (-1, ax)
    } else if ax < 0.6875 {
        (0, (2.0 * ax - 1.0) / (2.0 + ax))
    } else if ax < 1.1875 {
        (1, (ax - 1.0) / (ax + 1.0))
    } else if ax < 2.4375 {
        (2, (ax - 1.5) / (1.0 + 1.5 * ax))
    } else {
        (3, -1.0 / ax)
    };
    let z = t * t;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    if id < 0 {
        return sign * (t - t * (s1 + s2));
    }
    let id = id as usize;
    sign * (HI[id] - ((t * (s1 + s2) - LO[id]) - t))
}

/// The angle of the point (x, y), in (-π, π] (Table 28 feature 12).
pub fn atan2(y: f64, x: f64) -> f64 {
    if is_nan(x) || is_nan(y) {
        return f64::NAN;
    }
    let neg_y = bits(y) >> 63 == 1;
    let neg_x = bits(x) >> 63 == 1;
    let signed = |v: f64| if neg_y { -v } else { v };
    if y == 0.0 {
        // ±0 on the positive side, ±π on the negative (a -0 x counts as negative).
        return if neg_x { signed(PI + PI_LO) } else { y };
    }
    if x == 0.0 {
        return signed(PIO2_HI + PIO2_LO);
    }
    let (ax, ay) = (abs(x), abs(y));
    if ax == f64::INFINITY {
        return if ay == f64::INFINITY {
            signed(if neg_x { 3.0 * FRAC_PI_4 } else { FRAC_PI_4 })
        } else if neg_x {
            signed(PI + PI_LO)
        } else {
            signed(0.0)
        };
    }
    if ay == f64::INFINITY {
        return signed(PIO2_HI + PIO2_LO);
    }
    let ey = ((bits(ay) >> 52) & 0x7FF) as i32;
    let ex = ((bits(ax) >> 52) & 0x7FF) as i32;
    let z = if ey - ex > 60 {
        // |y/x| > 2^60.
        PIO2_HI + 0.5 * PIO2_LO
    } else if neg_x && ey - ex < -60 {
        0.0
    } else {
        atan(ay / ax)
    };
    let r = if neg_x { PI - (z - PI_LO) } else { z };
    signed(r)
}

/// asin x = atan(x / √((1 - x)(1 + x))); (1 - x)(1 + x) loses nothing near |x| = 1.
pub fn asin(x: f64) -> f64 {
    if is_nan(x) || abs(x) > 1.0 {
        return f64::NAN;
    }
    if abs(x) == 1.0 {
        return if x > 0.0 {
            PIO2_HI + PIO2_LO
        } else {
            -(PIO2_HI + PIO2_LO)
        };
    }
    atan2(x, sqrt((1.0 - x) * (1.0 + x)))
}

/// acos x = atan2(√((1 - x)(1 + x)), x).
pub fn acos(x: f64) -> f64 {
    if is_nan(x) || abs(x) > 1.0 {
        return f64::NAN;
    }
    atan2(sqrt((1.0 - x) * (1.0 + x)), x)
}

/// x rounded toward zero (TRUNC, IEC 61131-3 ed. 3 Table 22 footnote c).
pub fn trunc(x: f64) -> f64 {
    // From 2^52 on every double is an integer; NaN and the infinities are themselves.
    if is_nan(x) || abs(x) >= 4_503_599_627_370_496.0 {
        return x;
    }
    let t = x as i64 as f64;
    // -0.5 truncates to -0.
    if t == 0.0 && x < 0.0 {
        -0.0
    } else {
        t
    }
}

/// x to an integer power, by squaring: exact while the products are.
pub fn powi(x: f64, n: i64) -> f64 {
    let mut base = x;
    let mut e = n.unsigned_abs();
    let mut acc = 1.0;
    while e > 0 {
        if e & 1 == 1 {
            acc *= base;
        }
        base *= base;
        e >>= 1;
    }
    if n < 0 {
        1.0 / acc
    } else {
        acc
    }
}

/// x to the power y (EXPT, Table 29 feature 6): an integral y by squaring, otherwise
/// e^(y ln x) with ln x in two parts. A negative x to a y that is not integral is NaN.
pub fn pow(x: f64, y: f64) -> f64 {
    if is_nan(x) || is_nan(y) {
        return if y == 0.0 { 1.0 } else { f64::NAN };
    }
    if y == 0.0 {
        return 1.0;
    }
    let integral = abs(y) < 9.007_199_254_740_992e15 && round(y) == y;
    if integral && abs(y) <= 1024.0 {
        return powi(x, y as i64);
    }
    if x == 0.0 {
        return if y > 0.0 { 0.0 } else { f64::INFINITY };
    }
    let neg = x < 0.0;
    if neg && !integral {
        return f64::NAN;
    }
    // A square root, exactly, rather than through e^(ln x / 2).
    if y == 0.5 && x < f64::INFINITY {
        return sqrt(x);
    }
    let (hi, lo) = log_parts(abs(x));
    // y · (hi + lo) as t + u, then e^t · (1 + u).
    let (t, u) = two_prod(y, hi);
    let u = u + y * lo;
    let r = exp(t) * (1.0 + u);
    let odd = integral && {
        let half = y * 0.5;
        round(half) != half
    };
    if neg && odd {
        -r
    } else {
        r
    }
}
