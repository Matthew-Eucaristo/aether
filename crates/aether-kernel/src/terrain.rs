use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Terrain {
    pub seed: u32,
    pub relief: u32,
    pub scale: u32,
    pub plaza: u32,
}

const OCTAVES: u32 = 5;

fn wrap(x: f64) -> u32 {
    x.trunc().rem_euclid(4_294_967_296.0) as u32
}

fn lattice(x: f64, y: f64, seed: u32, octave: u32) -> f64 {
    let mut h = seed
        ^ wrap(x).wrapping_mul(0x27d4_eb2d)
        ^ wrap(y).wrapping_mul(0x1656_67b1)
        ^ octave.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 16;
    h as f64 / 4_294_967_296.0
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

fn noise(x: f64, y: f64, seed: u32, octave: u32) -> f64 {
    let (fx, fy) = (x.floor(), y.floor());
    let (u, v) = (smooth(x - fx), smooth(y - fy));
    let a = lattice(fx, fy, seed, octave);
    let b = lattice(fx + 1.0, fy, seed, octave);
    let c = lattice(fx, fy + 1.0, seed, octave);
    let d = lattice(fx + 1.0, fy + 1.0, seed, octave);
    let top = a + (b - a) * u;
    let bottom = c + (d - c) * u;
    top + (bottom - top) * v
}

/// Ground height in metres at (x, y). Uses only IEEE-754 operations that every platform and
/// JavaScript engine compute identically, so clients can reproduce it exactly.
pub fn height(t: &Terrain, x: f64, y: f64) -> f64 {
    if t.relief == 0 || !(x.is_finite() && y.is_finite()) {
        return 0.0;
    }
    let scale = f64::from(t.scale.max(1));
    let (mut px, mut py) = (x / scale, y / scale);
    let (mut sum, mut weight, mut total) = (0.0, 1.0, 0.0);
    for octave in 0..OCTAVES {
        sum += noise(px, py, t.seed, octave) * weight;
        total += weight;
        weight *= 0.5;
        px *= 2.0;
        py *= 2.0;
    }
    let mut h = f64::from(t.relief) * (2.0 * (sum / total) - 1.0);
    if t.plaza > 0 {
        let plaza = f64::from(t.plaza);
        let d = (x * x + y * y).sqrt();
        let k = ((d - plaza) / plaza).clamp(0.0, 1.0);
        h *= smooth(k);
    }
    h
}
