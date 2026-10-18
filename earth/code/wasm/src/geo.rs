//! Earth/Sun geometry. Scene frame: Earth-fixed, +X = (0°N, 0°E), +Y = north pole,
//! -Z = (0°N, 90°E) — this matches the UV layout of three.js `SphereGeometry`.

use core::f64::consts::PI;

pub const RE_KM: f64 = 6378.135;
pub const OMEGA_E: f64 = 7.292_115_146_706_979e-5; // rad/s
pub const DEG: f64 = PI / 180.0;

/// ECEF (km) → scene units (Earth radii).
#[inline]
pub fn ecef_to_scene(v: [f64; 3]) -> [f64; 3] {
    [v[0] / RE_KM, v[2] / RE_KM, -v[1] / RE_KM]
}

#[inline]
pub fn latlon_to_scene(lat_deg: f64, lon_deg: f64, r: f64) -> [f32; 3] {
    let (la, lo) = (lat_deg * DEG, lon_deg * DEG);
    [
        (r * la.cos() * lo.cos()) as f32,
        (r * la.sin()) as f32,
        (-r * la.cos() * lo.sin()) as f32,
    ]
}

#[inline]
pub fn unit(lat_deg: f64, lon_deg: f64) -> [f64; 3] {
    let (la, lo) = (lat_deg * DEG, lon_deg * DEG);
    [la.cos() * lo.cos(), la.cos() * lo.sin(), la.sin()]
}

/// Greenwich mean sidereal time (IAU-82, Vallado `gstime`), radians.
pub fn gmst(jd_ut1: f64) -> f64 {
    let t = (jd_ut1 - 2451545.0) / 36525.0;
    let mut temp = -6.2e-6 * t * t * t
        + 0.093104 * t * t
        + (876600.0 * 3600.0 + 8640184.812866) * t
        + 67310.54841;
    temp = (temp * DEG / 240.0) % (2.0 * PI);
    if temp < 0.0 {
        temp += 2.0 * PI;
    }
    temp
}

/// TEME/ECI → ECEF (polar motion ignored).
#[inline]
pub fn eci_to_ecef(v: [f64; 3], g: f64) -> [f64; 3] {
    let (s, c) = g.sin_cos();
    [c * v[0] + s * v[1], -s * v[0] + c * v[1], v[2]]
}

/// Low-precision solar direction (Astronomical Almanac), unit vector in ECEF.
pub fn sun_ecef(jd: f64) -> [f64; 3] {
    let n = jd - 2451545.0;
    let l = (280.460 + 0.9856474 * n) * DEG;
    let g = (357.528 + 0.9856003 * n) * DEG;
    let lam = l + (1.915 * g.sin() + 0.020 * (2.0 * g).sin()) * DEG;
    let eps = (23.439 - 0.0000004 * n) * DEG;
    let eci = [lam.cos(), eps.cos() * lam.sin(), eps.sin() * lam.sin()];
    eci_to_ecef(eci, gmst(jd))
}
