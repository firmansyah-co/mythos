//! The VM's own real functions (it has no libm: `mythos_logic_vm::math`) measured against
//! the host's, which is the platform's libm. The VM's give the same bits on every platform;
//! the libms do not, and are not exact either: a bound here holds against each libm CI runs
//! on (glibc on Linux, Apple's on macOS, Microsoft's on Windows). TAN is 2 ulp from glibc's
//! and 3 from Apple's at the same points. IEC 61131-3 ed. 3 §6.6.2.5.8 leaves their
//! accuracy to the implementer; this is where it is measured, and ADR-013 §2.13 states it.
//!
//! The points are drawn from a fixed seed: a failure is reproducible.

use mythos_logic_vm::math;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [lo, hi].
    fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (self.next() as f64 / u64::MAX as f64) * (hi - lo)
    }
    /// Spread over the decades between 10^lo and 10^hi.
    fn decades(&mut self, lo: f64, hi: f64) -> f64 {
        10f64.powf(self.uniform(lo, hi))
    }
}

/// How far `a` is from `b`, in units of the last place of `b`.
fn ulps(a: f64, b: f64) -> f64 {
    if a == b || (a.is_nan() && b.is_nan()) {
        return 0.0;
    }
    if !a.is_finite() || !b.is_finite() {
        return f64::INFINITY;
    }
    let b_abs = b.abs();
    let ulp = if b_abs == 0.0 {
        f64::from_bits(1)
    } else {
        f64::from_bits(b_abs.to_bits() + 1) - b_abs
    };
    (a - b).abs() / ulp
}

const POINTS: usize = 200_000;

/// The worst distance over `POINTS` points drawn by `draw`.
fn worst(
    draw: &mut dyn FnMut(&mut Rng) -> f64,
    mine: fn(f64) -> f64,
    host: fn(f64) -> f64,
) -> (f64, f64) {
    let mut rng = Rng(0x00C0_FFEE_0BAD_F00D);
    let mut w = (0.0, 0.0);
    for _ in 0..POINTS {
        let x = draw(&mut rng);
        let u = ulps(mine(x), host(x));
        if u > w.0 {
            w = (u, x);
        }
    }
    w
}

#[test]
fn each_function_of_one_argument_is_within_its_stated_ulps_of_the_host() {
    type Case = (
        &'static str,
        fn(f64) -> f64,
        fn(f64) -> f64,
        f64,
        f64,
        bool,
        f64,
    );
    // (name, ours, the host's, from, to, over decades, the bound in ulps)
    let cases: [Case; 18] = [
        ("SQRT", math::sqrt, f64::sqrt, -300.0, 300.0, true, 0.0),
        (
            "SQRT subnormal",
            math::sqrt,
            f64::sqrt,
            -323.0,
            -308.0,
            true,
            0.0,
        ),
        ("EXP", math::exp, f64::exp, -745.0, 709.0, false, 1.0),
        ("EXP near 0", math::exp, f64::exp, -1.0, 1.0, false, 1.0),
        ("LN", math::ln, f64::ln, -300.0, 300.0, true, 1.0),
        ("LN near 1", math::ln, f64::ln, 0.5, 2.0, false, 1.0),
        ("LOG", math::log10, f64::log10, -300.0, 300.0, true, 2.0),
        ("LOG near 1", math::log10, f64::log10, 0.5, 2.0, false, 2.0),
        ("SIN", math::sin, f64::sin, -10.0, 10.0, false, 1.0),
        ("SIN to 10^6", math::sin, f64::sin, -1e6, 1e6, false, 1.0),
        ("SIN past 10^7", math::sin, f64::sin, 7.0, 300.0, true, 1.0),
        ("COS", math::cos, f64::cos, -10.0, 10.0, false, 1.0),
        ("COS past 10^7", math::cos, f64::cos, 7.0, 300.0, true, 1.0),
        ("TAN", math::tan, f64::tan, -10.0, 10.0, false, 3.0),
        ("TAN past 10^7", math::tan, f64::tan, 7.0, 300.0, true, 3.0),
        ("ATAN", math::atan, f64::atan, -300.0, 300.0, true, 1.0),
        ("ASIN", math::asin, f64::asin, -1.0, 1.0, false, 2.0),
        ("ACOS", math::acos, f64::acos, -1.0, 1.0, false, 2.0),
    ];
    let mut over = Vec::new();
    for (name, mine, host, lo, hi, decades, bound) in cases {
        let mut draw = |r: &mut Rng| {
            if decades {
                r.decades(lo, hi)
            } else {
                r.uniform(lo, hi)
            }
        };
        let (u, at) = worst(&mut draw, mine, host);
        println!("{name:15} at most {u} ulp (worst at {at:e})");
        if u > bound {
            over.push(format!("{name}: {u} ulp at {at:e}, more than {bound}"));
        }
    }
    assert!(over.is_empty(), "{}", over.join("\n"));
}

#[test]
fn atan2_and_expt_are_within_their_stated_ulps_of_the_host() {
    let mut rng = Rng(0x5EED_0000_0000_0001);
    let (mut a, mut p, mut far) = (0.0f64, 0.0f64, 0.0f64);
    for _ in 0..POINTS {
        let (y, x) = (rng.uniform(-1e3, 1e3), rng.uniform(-1e3, 1e3));
        a = a.max(ulps(math::atan2(y, x), y.atan2(x)));
        let (x, y) = (rng.uniform(1e-9, 1e5), rng.uniform(-20.0, 20.0));
        p = p.max(ulps(math::pow(x, y), x.powf(y)));
        // Results near the ends of the range: e^(y ln x) with y ln x near ±700.
        let (x, y) = (rng.uniform(0.5, 2.0), rng.uniform(-700.0, 700.0));
        let want = x.powf(y);
        if want.is_normal() {
            far = far.max(ulps(math::pow(x, y), want));
        }
    }
    println!("ATAN2 at most {a} ulp; EXPT at most {p} ulp, {far} near the ends of the range");
    assert!(a <= 1.0, "ATAN2: {a}");
    assert!(p <= 4.0, "EXPT: {p}");
    assert!(far <= 128.0, "EXPT near the ends of the range: {far}");
}

#[test]
fn what_is_not_a_real_number_is_given_as_iec_60559_gives_it() {
    let inf = f64::INFINITY;
    assert!(math::sqrt(-1.0).is_nan());
    assert!(math::ln(-1.0).is_nan());
    assert!(math::log10(-1.0).is_nan());
    assert!(math::asin(1.5).is_nan());
    assert!(math::acos(-1.5).is_nan());
    assert!(math::pow(-8.0, 0.5).is_nan());
    assert_eq!(math::ln(0.0), -inf);
    assert_eq!(math::exp(1000.0), inf);
    assert_eq!(math::exp(-1000.0), 0.0);
    assert_eq!(math::sqrt(inf), inf);
    assert_eq!(math::atan(inf), std::f64::consts::FRAC_PI_2);
    assert!(math::sin(inf).is_nan());
    // Exact where the host's is: an integral power by squaring, a power of ten's logarithm.
    assert_eq!(math::pow(2.0, 3.0), 8.0);
    assert_eq!(math::pow(-2.0, 3.0), -8.0);
    assert_eq!(math::pow(2.0, -2.0), 0.25);
    assert_eq!(math::pow(0.0, -1.0), inf);
    assert_eq!(math::log10(1000.0), 3.0);
    assert_eq!(math::sqrt(2.0), std::f64::consts::SQRT_2);
    assert_eq!(math::exp(-740.0), (-740.0f64).exp(), "into the subnormals");
}
