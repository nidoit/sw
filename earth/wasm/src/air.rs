//! Air-quality layer: station/model points (lat, lon, US-AQI) → interpolated equirectangular
//! heat-map texture and an advected "dust" particle system coloured by local AQI.

use crate::geo;
use crate::rng::Rng;
use crate::scan;

pub const PSTRIDE: usize = 5; // x, y, z, aqi, alpha
const BIN: f64 = 5.0;
const BW: usize = 72;
const BH: usize = 36;

#[derive(Default)]
pub struct Air {
    pub pts: Vec<[f32; 3]>,
    pub grid: Vec<f32>, // AQI per texel, NaN = no data
    pub gw: usize,
    pub gh: usize,
    pub tex: Vec<u8>,
    parts: Vec<[f32; 4]>, // lat, lon, age, life
    pub out: Vec<f32>,
    rng: Option<Rng>,
    clock: f64,
}

/// US EPA AQI colour ramp.
pub fn aqi_rgb(a: f32) -> [f32; 3] {
    const STOPS: [(f32, [f32; 3]); 7] = [
        (0.0, [0.0, 0.85, 0.35]),
        (50.0, [0.35, 0.9, 0.0]),
        (100.0, [1.0, 0.92, 0.0]),
        (150.0, [1.0, 0.5, 0.0]),
        (200.0, [1.0, 0.0, 0.0]),
        (300.0, [0.56, 0.25, 0.6]),
        (400.0, [0.5, 0.0, 0.14]),
    ];
    if a <= 0.0 {
        return STOPS[0].1;
    }
    for w in STOPS.windows(2) {
        if a <= w[1].0 {
            let t = (a - w[0].0) / (w[1].0 - w[0].0);
            return [0, 1, 2].map(|k| w[0].1[k] + (w[1].1[k] - w[0].1[k]) * t);
        }
    }
    STOPS[6].1
}

impl Air {
    pub fn clear(&mut self) {
        self.pts.clear();
    }

    fn add(&mut self, lat: f64, lon: f64, aqi: f64) {
        if lat.is_finite() && lon.is_finite() && aqi.is_finite() && aqi >= 0.0 && aqi < 1000.0 {
            self.pts.push([lat as f32, lon as f32, aqi as f32]);
        }
    }

    /// Open-Meteo (Copernicus CAMS) multi-location response: objects carrying
    /// `"latitude"`, `"longitude"` and `"current":{..., "us_aqi": N}`.
    pub fn parse_openmeteo(&mut self, b: &[u8]) -> u32 {
        let before = self.pts.len();
        let mut pos = 0;
        while let Some(i) = scan::find(b, b"\"latitude\"", pos) {
            let (lat, p) = scan::number(b, i + 10);
            let Some(j) = scan::find(b, b"\"longitude\"", p) else { break };
            let (lon, p) = scan::number(b, j + 11);
            let Some(k) = scan::find(b, b"\"current\":", p) else { break };
            let next_obj = scan::find(b, b"\"latitude\"", k).unwrap_or(b.len());
            pos = k + 10;
            if let Some(q) = scan::find(b, b"\"us_aqi\"", k) {
                if q < next_obj {
                    let (v, _) = scan::number(b, q + 8);
                    if let (Some(la), Some(lo), Some(v)) = (lat, lon, v) {
                        self.add(la, lo, v);
                    }
                }
            }
        }
        (self.pts.len() - before) as u32
    }

    /// WAQI `/v2/map/bounds` response: `{"lat":..,"lon":..,"uid":..,"aqi":"57",...}`.
    pub fn parse_waqi(&mut self, b: &[u8]) -> u32 {
        let before = self.pts.len();
        let mut pos = 0;
        while let Some(i) = scan::find(b, b"\"lat\"", pos) {
            let (lat, p) = scan::number(b, i + 5);
            let Some(j) = scan::find(b, b"\"lon\"", p) else { break };
            let (lon, p) = scan::number(b, j + 5);
            let Some(k) = scan::find(b, b"\"aqi\"", p) else { break };
            let (v, p) = scan::number(b, k + 5);
            pos = p;
            if let (Some(la), Some(lo), Some(v)) = (lat, lon, v) {
                self.add(la, lo, v);
            }
        }
        (self.pts.len() - before) as u32
    }

    /// Offline fallback: typical annual AQI of large urban regions over a clean background.
    pub fn synth(&mut self, seed: u64) -> u32 {
        const HOT: [(f64, f64, f64); 30] = [
            (28.6, 77.2, 175.0), (31.5, 74.3, 180.0), (23.8, 90.4, 165.0), (39.9, 116.4, 115.0),
            (31.2, 121.5, 95.0), (30.6, 104.1, 110.0), (22.5, 88.4, 150.0), (19.1, 72.9, 120.0),
            (24.9, 67.0, 160.0), (30.0, 31.2, 145.0), (6.5, 3.4, 130.0), (35.7, 51.4, 120.0),
            (24.7, 46.7, 135.0), (33.3, 44.4, 150.0), (-6.2, 106.8, 120.0), (13.8, 100.5, 95.0),
            (21.0, 105.8, 115.0), (14.6, 121.0, 80.0), (19.4, -99.1, 95.0), (-23.5, -46.6, 70.0),
            (34.1, -118.2, 75.0), (40.7, -74.0, 50.0), (51.5, -0.1, 45.0), (48.9, 2.4, 45.0),
            (52.2, 21.0, 60.0), (41.0, 29.0, 70.0), (55.8, 37.6, 55.0), (37.6, 127.0, 75.0),
            (35.7, 139.7, 50.0), (57.7, 12.0, 25.0),
        ];
        let mut r = Rng::new(seed);
        let before = self.pts.len();
        for lat in (-60..=75).step_by(10) {
            for lon in (-180..180).step_by(10) {
                let v = 18.0 + r.f() * 20.0 + if (0..=40).contains(&lat) && (-20..=60).contains(&lon) { 35.0 } else { 0.0 };
                self.add(lat as f64, lon as f64, v);
            }
        }
        for (la, lo, v) in HOT {
            self.add(la, lo, v);
            for _ in 0..6 {
                self.add(la + r.gauss(0.0, 2.5), lo + r.gauss(0.0, 2.5), v * r.range(0.55, 0.9));
            }
        }
        (self.pts.len() - before) as u32
    }

    /// Inverse-distance interpolation on the sphere using 5° buckets. Returns the RGBA texture.
    pub fn build(&mut self, w: usize, h: usize) -> &[u8] {
        self.gw = w;
        self.gh = h;
        self.grid.clear();
        self.grid.resize(w * h, f32::NAN);
        self.tex.clear();
        self.tex.resize(w * h * 4, 0);
        let mut bins: Vec<Vec<u32>> = vec![Vec::new(); BW * BH];
        let vecs: Vec<[f64; 3]> = self.pts.iter().map(|p| geo::unit(p[0] as f64, p[1] as f64)).collect();
        for (i, p) in self.pts.iter().enumerate() {
            let by = (((p[0] as f64 + 90.0) / BIN) as usize).min(BH - 1);
            let bx = (((p[1] as f64 + 180.0).rem_euclid(360.0) / BIN) as usize).min(BW - 1);
            bins[by * BW + bx].push(i as u32);
        }
        let reach = 3i64; // ±15°
        let r_max2 = (16.0f64 * geo::DEG).powi(2);
        for y in 0..h {
            let lat = 90.0 - (y as f64 + 0.5) * 180.0 / h as f64;
            let by = ((lat + 90.0) / BIN) as i64;
            let lon_reach = (reach as f64 / (lat * geo::DEG).cos().max(0.2)).ceil() as i64;
            for x in 0..w {
                let lon = -180.0 + (x as f64 + 0.5) * 360.0 / w as f64;
                let bx = ((lon + 180.0) / BIN) as i64;
                let u = geo::unit(lat, lon);
                let (mut sw, mut sv) = (0.0f64, 0.0f64);
                for dy in -reach..=reach {
                    let yy = by + dy;
                    if !(0..BH as i64).contains(&yy) {
                        continue;
                    }
                    for dx in -lon_reach.min(36)..=lon_reach.min(36) {
                        let xx = (bx + dx).rem_euclid(BW as i64);
                        for &pi in &bins[yy as usize * BW + xx as usize] {
                            let q = vecs[pi as usize];
                            let d2 = (u[0] - q[0]).powi(2) + (u[1] - q[1]).powi(2) + (u[2] - q[2]).powi(2);
                            if d2 > r_max2 {
                                continue;
                            }
                            let wgt = (1.0 - d2 / r_max2).powi(2) / (d2 + 2e-4);
                            sw += wgt;
                            sv += wgt * self.pts[pi as usize][2] as f64;
                        }
                    }
                }
                let idx = y * w + x;
                if sw > 0.0 {
                    let a = (sv / sw) as f32;
                    self.grid[idx] = a;
                    let c = aqi_rgb(a);
                    let conf = (sw / (sw + 40.0)) as f32;
                    let alpha = (0.12 + 0.45 * (a / 220.0).min(1.0)) * conf.max(0.35);
                    self.tex[idx * 4..idx * 4 + 4].copy_from_slice(&[
                        (c[0] * 255.0) as u8,
                        (c[1] * 255.0) as u8,
                        (c[2] * 255.0) as u8,
                        (alpha * 255.0) as u8,
                    ]);
                }
            }
        }
        &self.tex
    }

    pub fn sample(&self, lat: f64, lon: f64) -> f32 {
        if self.grid.is_empty() {
            return f32::NAN;
        }
        let x = (((lon + 180.0).rem_euclid(360.0)) / 360.0 * self.gw as f64) as usize;
        let y = ((90.0 - lat) / 180.0 * self.gh as f64) as usize;
        self.grid[y.min(self.gh - 1) * self.gw + x.min(self.gw - 1)]
    }

    fn spawn(&mut self) -> [f32; 4] {
        let r = self.rng.as_mut().unwrap();
        let mut best = [0.0f32, 0.0, 0.0, 1.0];
        for _ in 0..24 {
            let lat = (r.range(-1.0, 1.0)).asin() / geo::DEG; // area-uniform
            let lon = r.range(-180.0, 180.0);
            let life = r.range(6.0, 14.0) as f32;
            best = [lat as f32, lon as f32, 0.0, life];
            let gx = (((lon + 180.0) / 360.0) * self.gw as f64) as usize;
            let gy = (((90.0 - lat) / 180.0) * self.gh as f64) as usize;
            let a = self.grid[gy.min(self.gh - 1) * self.gw + gx.min(self.gw - 1)];
            if a.is_finite() && r.f() < ((a / 160.0).powf(1.4) as f64).clamp(0.02, 1.0) {
                break;
            }
        }
        best
    }

    pub fn init_particles(&mut self, n: usize, seed: u64) {
        self.rng = Some(Rng::new(seed));
        self.parts.clear();
        self.out.clear();
        self.out.resize(n * PSTRIDE, 0.0);
        if self.grid.is_empty() {
            return;
        }
        for _ in 0..n {
            let mut p = self.spawn();
            p[2] = self.rng.as_mut().unwrap().range(0.0, p[3] as f64) as f32;
            self.parts.push(p);
        }
    }

    /// Advects particles with an idealised three-cell zonal wind (trades, westerlies, polar
    /// easterlies) plus divergence-free swirl. `dt` in wall seconds, `speed` = visual time scale.
    pub fn step(&mut self, dt: f64, speed: f64) {
        if self.parts.is_empty() {
            return;
        }
        self.clock += dt;
        let t = self.clock;
        let n = self.parts.len();
        for i in 0..n {
            let mut p = self.parts[i];
            p[2] += dt as f32;
            if p[2] > p[3] {
                p = self.spawn();
            }
            let lat = p[0] as f64;
            let lon = p[1] as f64;
            let la = lat * geo::DEG;
            let lo = lon * geo::DEG;
            let u = -8.0 * (6.0 * la.abs()).sin() + 3.0 * (3.0 * lo + 2.0 * la + 0.05 * t).sin() * (2.0 * la).cos();
            let v = 2.5 * (2.0 * lo - 0.04 * t).cos() * (3.0 * la).sin() + 1.5 * (5.0 * lo + 4.0 * la).sin();
            let k = dt * speed;
            let dlon = u * k / (111_320.0 * la.cos().max(0.08));
            let dlat = v * k / 110_540.0;
            let mut nlat = lat + dlat;
            if nlat.abs() > 89.0 {
                nlat = nlat.signum() * 89.0;
            }
            p[0] = nlat as f32;
            p[1] = ((lon + dlon + 180.0).rem_euclid(360.0) - 180.0) as f32;
            self.parts[i] = p;

            let a = self.sample(p[0] as f64, p[1] as f64);
            let fade = (p[2] / 1.5).min(1.0).min((p[3] - p[2]) / 1.5).max(0.0);
            let hgt = 1.006 + 0.006 * ((i * 7919 % 1000) as f64 / 1000.0);
            let xyz = geo::latlon_to_scene(p[0] as f64, p[1] as f64, hgt);
            let o = i * PSTRIDE;
            self.out[o..o + PSTRIDE].copy_from_slice(&[xyz[0], xyz[1], xyz[2], if a.is_finite() { a } else { 0.0 }, fade]);
        }
    }

    pub fn stats(&self, out: &mut [f32]) {
        let n = self.pts.len() as f32;
        let (mut s, mut m) = (0.0f32, 0.0f32);
        for p in &self.pts {
            s += p[2];
            m = m.max(p[2]);
        }
        out[0] = n;
        out[1] = if n > 0.0 { s / n } else { 0.0 };
        out[2] = m;
    }
}

/// Web-Mercator (EPSG:3857, |lat| ≤ 85.05°) RGBA → equirectangular RGBA, bilinear.
pub fn merc_to_equirect(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    let mut out = vec![0u8; dw * dh * 4];
    let lim = 85.051_128f64;
    for y in 0..dh {
        let lat = 90.0 - (y as f64 + 0.5) * 180.0 / dh as f64;
        if lat.abs() > lim {
            continue;
        }
        let phi = lat * geo::DEG;
        let my = (1.0 - (core::f64::consts::FRAC_PI_4 + phi / 2.0).tan().ln() / core::f64::consts::PI) / 2.0 * sh as f64 - 0.5;
        let y0 = (my.floor().max(0.0) as usize).min(sh - 1);
        let y1 = (y0 + 1).min(sh - 1);
        let fy = (my - my.floor()).clamp(0.0, 1.0);
        for x in 0..dw {
            let mx = (x as f64 + 0.5) / dw as f64 * sw as f64 - 0.5;
            let x0 = (mx.floor().rem_euclid(sw as f64)) as usize;
            let x1 = (x0 + 1) % sw;
            let fx = mx - mx.floor();
            for c in 0..4 {
                let s = |xx: usize, yy: usize| src[(yy * sw + xx) * 4 + c] as f64;
                let v = (s(x0, y0) * (1.0 - fx) + s(x1, y0) * fx) * (1.0 - fy) + (s(x0, y1) * (1.0 - fx) + s(x1, y1) * fx) * fy;
                out[(y * dw + x) * 4 + c] = v.round() as u8;
            }
        }
    }
    out
}
