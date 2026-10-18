//! Planetary-boundary layers: SST anomaly, active fires (NASA FIRMS CSV), sea-ice extent
//! (NSIDC Sea Ice Index) and near-Earth-object flybys (NASA NeoWs JSON).

use crate::geo::{self, DEG};
use crate::rng::Rng;
use crate::scan;
use core::f64::consts::PI;

pub const FIRE_STRIDE: usize = 5; // x, y, z, frp norm, phase
pub const ICE_RINGS: usize = 12;
pub const ICE_SEG: usize = 180;
pub const NEO_PTS: usize = 96;

// ---------------------------------------------------------------- noise
fn hash2(x: i64, y: i64, s: u64) -> f64 {
    let mut h = (x as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F) ^ s;
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58476D1CE4E5B9);
    h ^= h >> 29;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

fn vnoise(x: f64, y: f64, s: u64) -> f64 {
    let (xi, yi) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = hash2(xi, yi, s);
    let b = hash2(xi + 1, yi, s);
    let c = hash2(xi, yi + 1, s);
    let d = hash2(xi + 1, yi + 1, s);
    (a + (b - a) * ux) * (1.0 - uy) + (c + (d - c) * ux) * uy
}

fn fbm(x: f64, y: f64, s: u64) -> f64 {
    let (mut v, mut amp, mut f) = (0.0, 0.5, 1.0);
    for o in 0..5 {
        v += amp * vnoise(x * f, y * f, s + o);
        amp *= 0.5;
        f *= 2.0;
    }
    v
}

/// Diverging blue–white–red ramp for anomalies in °C (±3 saturates).
pub fn anomaly_rgb(a: f64) -> [u8; 3] {
    let t = (a / 3.0).clamp(-1.0, 1.0);
    let (r, g, b) = if t >= 0.0 {
        (1.0, 1.0 - 0.85 * t, 1.0 - 0.95 * t)
    } else {
        (1.0 + 0.9 * t, 1.0 + 0.55 * t, 1.0)
    };
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}

/// Procedural SST anomaly field (fallback when NASA GIBS is unreachable): global warming offset,
/// North-Atlantic and NE-Pacific marine heatwaves, ENSO tongue, mesoscale eddies.
pub fn sst_synth(w: usize, h: usize, enso: f64, seed: u64) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    let blob = |lat: f64, lon: f64, la0: f64, lo0: f64, sla: f64, slo: f64| {
        let dlo = ((lon - lo0 + 540.0) % 360.0) - 180.0;
        (-((lat - la0) / sla).powi(2) - (dlo / slo).powi(2)).exp()
    };
    for y in 0..h {
        let lat = 90.0 - (y as f64 + 0.5) * 180.0 / h as f64;
        for x in 0..w {
            let lon = -180.0 + (x as f64 + 0.5) * 360.0 / w as f64;
            let mut a = 0.45 + 0.25 * (lat * DEG).cos();
            a += 1.6 * blob(lat, lon, 45.0, -40.0, 12.0, 25.0);
            a += 1.2 * blob(lat, lon, 42.0, -150.0, 10.0, 25.0);
            a += 0.9 * blob(lat, lon, 38.0, 18.0, 6.0, 15.0);
            a += enso * blob(lat, lon, 0.0, -120.0, 6.0, 45.0);
            a -= 0.6 * blob(lat, lon, -58.0, 0.0, 6.0, 180.0);
            a += 1.4 * (fbm(lon / 12.0, lat / 12.0, seed) - 0.5) * 2.0;
            let c = anomaly_rgb(a);
            let i = (y * w + x) * 4;
            out[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], (120.0 + 120.0 * (a.abs() / 3.0).min(1.0)) as u8]);
        }
    }
    out
}

// ---------------------------------------------------------------- fires
#[derive(Default)]
pub struct Fires {
    pts: Vec<[f32; 3]>, // lat, lon, frp
    pub out: Vec<f32>,
}

impl Fires {
    pub fn clear(&mut self) {
        self.pts.clear();
    }

    /// FIRMS area CSV: header row names `latitude`, `longitude` and `frp` (or `bright_ti4`).
    pub fn parse_csv(&mut self, b: &[u8]) -> u32 {
        let before = self.pts.len();
        let mut lines = b.split(|&c| c == b'\n');
        let Some(head) = lines.next() else { return 0 };
        let cols: Vec<&[u8]> = head.split(|&c| c == b',').map(|c| c.strip_suffix(b"\r").unwrap_or(c)).collect();
        let idx = |name: &[u8]| cols.iter().position(|c| *c == name);
        let (Some(ila), Some(ilo)) = (idx(b"latitude"), idx(b"longitude")) else { return 0 };
        let ifr = idx(b"frp");
        for line in lines {
            let f: Vec<&[u8]> = line.split(|&c| c == b',').collect();
            if f.len() <= ila.max(ilo) {
                continue;
            }
            let la = scan::number(f[ila], 0).0;
            let lo = scan::number(f[ilo], 0).0;
            let frp = ifr.and_then(|i| f.get(i)).and_then(|s| scan::number(s, 0).0).unwrap_or(10.0);
            if let (Some(la), Some(lo)) = (la, lo) {
                self.pts.push([la as f32, lo as f32, frp as f32]);
            }
        }
        (self.pts.len() - before) as u32
    }

    /// Offline fallback: seasonal fire belts, clustered.
    pub fn synth(&mut self, doy: f64, seed: u64) -> u32 {
        // (lat, lon, spread deg, peak day-of-year, weight)
        const BELTS: [(f64, f64, f64, f64, f64); 10] = [
            (-8.0, -60.0, 6.0, 240.0, 1.0),
            (-10.0, 22.0, 7.0, 210.0, 1.4),
            (8.0, 10.0, 6.0, 15.0, 1.2),
            (60.0, 110.0, 9.0, 200.0, 0.8),
            (57.0, -115.0, 7.0, 210.0, 0.6),
            (-20.0, 135.0, 9.0, 300.0, 0.7),
            (18.0, 100.0, 5.0, 75.0, 0.7),
            (38.0, -121.0, 3.0, 240.0, 0.3),
            (-1.0, 113.0, 3.0, 260.0, 0.4),
            (40.0, 20.0, 5.0, 220.0, 0.3),
        ];
        let mut r = Rng::new(seed);
        let before = self.pts.len();
        for (la, lo, sp, peak, wgt) in BELTS {
            let season = (0.5 + 0.5 * (2.0 * PI * (doy - peak) / 365.0).cos()).powi(2);
            let clusters = (60.0 * wgt * season) as usize + 2;
            for _ in 0..clusters {
                let (cla, clo) = (r.gauss(la, sp * 0.6), r.gauss(lo, sp));
                for _ in 0..(r.range(3.0, 30.0) as usize) {
                    self.pts.push([r.gauss(cla, 0.25) as f32, r.gauss(clo, 0.25) as f32, (r.f().powi(3) * 150.0 + 2.0) as f32]);
                }
            }
        }
        (self.pts.len() - before) as u32
    }

    pub fn build(&mut self) -> u32 {
        self.out.clear();
        for (i, p) in self.pts.iter().enumerate() {
            let xyz = geo::latlon_to_scene(p[0] as f64, p[1] as f64, 1.003);
            let frp = ((p[2].max(0.5)).ln() / 6.0).clamp(0.05, 1.0);
            let phase = (i as f32 * 0.618_034).fract() * 6.283;
            self.out.extend_from_slice(&[xyz[0], xyz[1], xyz[2], frp, phase]);
        }
        self.pts.len() as u32
    }
}

// ---------------------------------------------------------------- sea ice
/// 1981–2010 median daily extent (10⁶ km²) approximated by an annual harmonic.
pub fn ice_baseline(north: bool, doy: f64) -> f64 {
    if north {
        10.95 + 4.65 * (2.0 * PI * (doy - 70.0) / 365.25).cos()
    } else {
        10.8 + 7.9 * (2.0 * PI * (doy - 263.0) / 365.25).cos()
    }
}

/// Trend model used when NSIDC is unreachable (Arctic loss strongest in late summer).
pub fn ice_model(north: bool, doy: f64, year: f64) -> f64 {
    let base = ice_baseline(north, doy);
    if north {
        let summer = (0.5 - 0.5 * (2.0 * PI * (doy - 70.0) / 365.25).cos()).max(0.0);
        (base - (year - 1995.0) * (0.035 + 0.055 * summer)).max(2.5)
    } else {
        (base - 1.1).max(1.5)
    }
}

/// Last row of an NSIDC `N_seaice_extent_daily_v3.0.csv` style file → (extent, year, doy).
pub fn parse_ice_csv(b: &[u8]) -> Option<(f64, f64, f64)> {
    let mut best = None;
    for line in b.split(|&c| c == b'\n') {
        let f: Vec<&[u8]> = line.split(|&c| c == b',').collect();
        if f.len() < 4 {
            continue;
        }
        let (Some(y), Some(m), Some(d), Some(e)) =
            (scan::number(f[0], 0).0, scan::number(f[1], 0).0, scan::number(f[2], 0).0, scan::number(f[3], 0).0)
        else {
            continue;
        };
        if y > 1900.0 && (1.0..=12.0).contains(&m) && e > 0.0 {
            const CUM: [f64; 12] = [0.0, 31.0, 59.0, 90.0, 120.0, 151.0, 181.0, 212.0, 243.0, 273.0, 304.0, 334.0];
            best = Some((e, y, CUM[m as usize - 1] + d));
        }
    }
    best
}

/// Polar-cap angular radius (deg) whose ocean-covered area matches `extent`.
fn cap_radius(north: bool, extent: f64) -> f64 {
    const R: f64 = 6371.0;
    let full = 2.0 * PI * R * R / 1e6; // 10⁶ km² per unit (1 - cos r)
    let cap_area = if north { extent / 0.62 } else { extent + 13.9 };
    (1.0 - (cap_area / full).min(1.0)).acos() / DEG
}

/// Writes a polar-cap mesh (rings × segments vertices, xyz) whose edge follows a
/// longitude-dependent radius (climatological asymmetry). Returns vertex floats.
pub fn ice_cap(out: &mut Vec<f32>, north: bool, extent: f64, height: f64) {
    let r0 = cap_radius(north, extent);
    let sgn = if north { 1.0 } else { -1.0 };
    for ring in 0..=ICE_RINGS {
        let f = ring as f64 / ICE_RINGS as f64;
        for s in 0..=ICE_SEG {
            let lon = -180.0 + 360.0 * s as f64 / ICE_SEG as f64;
            let l = lon * DEG;
            let shape = if north {
                1.0 + 0.22 * (l + 0.9).cos() + 0.10 * (2.0 * l - 0.6).cos()
            } else {
                1.0 + 0.12 * (l - 2.2).cos() + 0.08 * (3.0 * l).cos()
            };
            let colat = r0 * shape * f;
            let p = geo::latlon_to_scene(sgn * (90.0 - colat), lon, height);
            out.extend_from_slice(&p);
        }
    }
}

// ---------------------------------------------------------------- near-Earth objects
pub struct Neo {
    pub name: Vec<u8>,
    pub hazardous: bool,
    pub dia_km: f64,
    pub ca_ms: f64,
    pub v_kms: f64,
    pub miss_km: f64,
}

#[derive(Default)]
pub struct Neos {
    pub list: Vec<Neo>,
    pub lines: Vec<f32>, // NEO_PTS points per object, xyz
    pub now: Vec<f32>,   // current position per object, xyz
}

/// Display mapping for distances far beyond GEO: log-compressed radius in Earth radii.
pub fn compress(d_km: f64) -> f64 {
    1.0 + 2.2 * (1.0 + d_km / geo::RE_KM).log10()
}

impl Neos {
    /// NASA NeoWs `/feed` JSON. Returns objects read.
    pub fn parse(&mut self, b: &[u8]) -> u32 {
        self.list.clear();
        let key = b"\"neo_reference_id\"";
        let mut starts = Vec::new();
        let mut p = 0;
        while let Some(i) = scan::find(b, key, p) {
            starts.push(i);
            p = i + key.len();
        }
        for (k, &s) in starts.iter().enumerate() {
            let e = starts.get(k + 1).copied().unwrap_or(b.len());
            let seg = &b[s..e];
            let name = scan::find(seg, b"\"name\"", 0)
                .and_then(|i| {
                    let a = scan::find(seg, b"\"", i + 6)? + 1;
                    let z = scan::find(seg, b"\"", a)?;
                    Some(seg[a..z].to_vec())
                })
                .unwrap_or_default();
            let num_after = |pat: &[u8], from: usize| scan::find(seg, pat, from).and_then(|i| scan::number(seg, i + pat.len()).0);
            let dia = num_after(b"\"estimated_diameter_max\"", 0).unwrap_or(0.05);
            let hazardous = scan::find(seg, b"\"is_potentially_hazardous_asteroid\"", 0)
                .map(|i| scan::find(&seg[i..(i + 45).min(seg.len())], b"true", 0).is_some())
                .unwrap_or(false);
            let ca = num_after(b"\"epoch_date_close_approach\"", 0).unwrap_or(0.0);
            let v = num_after(b"\"kilometers_per_second\"", 0).unwrap_or(10.0);
            let miss = scan::find(seg, b"\"miss_distance\"", 0).and_then(|i| num_after(b"\"kilometers\"", i)).unwrap_or(1e7);
            self.list.push(Neo { name, hazardous, dia_km: dia, ca_ms: ca, v_kms: v, miss_km: miss });
        }
        self.list.len() as u32
    }

    /// Schematic straight-line flyby (feed carries no orbit elements; geometry is seeded per object).
    pub fn build(&mut self, now_ms: f64) {
        self.lines.clear();
        self.now.clear();
        for (k, n) in self.list.iter().enumerate() {
            let mut r = Rng::new(0x5EED + k as u64 * 7919 + n.name.iter().map(|&c| c as u64).sum::<u64>());
            // random unit normal b̂ and perpendicular direction v̂
            let z = r.range(-1.0, 1.0);
            let t = r.range(0.0, 2.0 * PI);
            let bh = [(1.0 - z * z).sqrt() * t.cos(), z, (1.0 - z * z).sqrt() * t.sin()];
            let mut a = [r.gauss(0.0, 1.0), r.gauss(0.0, 1.0), r.gauss(0.0, 1.0)];
            let d = a[0] * bh[0] + a[1] * bh[1] + a[2] * bh[2];
            for i in 0..3 {
                a[i] -= d * bh[i];
            }
            let al = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-9);
            let vh = a.map(|c| c / al);
            let span_s = 4.0 * 86400.0;
            let place = |ts: f64| {
                let p = [0, 1, 2].map(|i| bh[i] * n.miss_km + vh[i] * n.v_kms * ts);
                let dist = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
                let rr = compress(dist) / dist;
                [(p[0] * rr) as f32, (p[1] * rr) as f32, (p[2] * rr) as f32]
            };
            for i in 0..NEO_PTS {
                let ts = -span_s + 2.0 * span_s * i as f64 / (NEO_PTS - 1) as f64;
                self.lines.extend_from_slice(&place(ts));
            }
            let ts = ((now_ms - n.ca_ms) / 1000.0).clamp(-span_s, span_s);
            self.now.extend_from_slice(&place(ts));
        }
    }

    pub fn info(&self, i: usize, out: &mut [f32]) -> bool {
        let Some(n) = self.list.get(i) else { return false };
        out[0] = n.miss_km as f32;
        out[1] = (n.miss_km / 384_400.0) as f32;
        out[2] = n.v_kms as f32;
        out[3] = n.dia_km as f32;
        out[4] = n.hazardous as u8 as f32;
        // ms epoch split to keep f32 precision: days and remainder seconds
        out[5] = (n.ca_ms / 86_400_000.0).floor() as f32;
        out[6] = ((n.ca_ms % 86_400_000.0) / 1000.0) as f32;
        true
    }
}
