//! NOAA SWPC OVATION Prime grid → aurora curtain particles (100–300 km altitude).
//! Particle layout (stride 5): x, y, z, intensity [0..1], altitude fraction [0..1].

use crate::geo::{self, DEG};
use crate::rng::Rng;
use crate::scan;

pub const W: usize = 360;
pub const H: usize = 181;
pub const STRIDE: usize = 5;

pub struct Aurora {
    pub grid: Vec<f32>,
    pub parts: Vec<f32>,
    pub max: f32,
    pub hemi_power: [f32; 2], // summed probability north / south (relative activity)
}

impl Default for Aurora {
    fn default() -> Self {
        Aurora { grid: vec![0.0; W * H], parts: Vec::new(), max: 0.0, hemi_power: [0.0; 2] }
    }
}

impl Aurora {
    /// Parses `{"coordinates": [[lon, lat, value], ...]}`. Returns cells read.
    pub fn parse(&mut self, text: &[u8]) -> u32 {
        let Some(start) = scan::find(text, b"\"coordinates\"", 0) else { return 0 };
        self.grid.fill(0.0);
        let mut pos = start + 13;
        let mut cells = 0;
        loop {
            // each entry starts with '['
            let Some(open) = scan::find(text, b"[", pos) else { break };
            let (lon, p1) = scan::number(text, open + 1);
            let (lat, p2) = scan::number(text, p1);
            let (val, p3) = scan::number(text, p2);
            pos = p3;
            match (lon, lat, val) {
                (Some(lon), Some(lat), Some(v)) => {
                    let x = (lon.round() as i64).rem_euclid(W as i64) as usize;
                    let y = ((lat.round() as i64) + 90).clamp(0, H as i64 - 1) as usize;
                    self.grid[y * W + x] = v as f32;
                    cells += 1;
                }
                _ => {
                    if lon.is_none() {
                        break;
                    }
                }
            }
        }
        cells
    }

    /// Kp-driven statistical auroral oval (fallback when SWPC is unreachable).
    pub fn synth(&mut self, kp: f64, jd: f64) {
        let sun = geo::sun_ecef(jd);
        let pn = geo::unit(80.65, -72.68);
        let theta0 = 17.0 + 1.7 * kp;
        let sigma = 2.2 + 0.45 * kp;
        let peak = 18.0 + 9.0 * kp;
        for y in 0..H {
            let lat = y as f64 - 90.0;
            for x in 0..W {
                let lon = x as f64;
                let u = geo::unit(lat, lon);
                let d = (u[0] * pn[0] + u[1] * pn[1] + u[2] * pn[2]).clamp(-1.0, 1.0);
                // colatitude from the nearest geomagnetic pole
                let colat = d.abs().acos() / DEG;
                let night = (0.5 - 1.2 * (u[0] * sun[0] + u[1] * sun[1] + u[2] * sun[2])).clamp(0.0, 1.0);
                // oval is shifted ~4° equatorward on the night side
                let c = theta0 + 4.0 * night;
                let g = (-((colat - c) / sigma).powi(2)).exp();
                self.grid[y * W + x] = (peak * g * (0.35 + 0.65 * night)) as f32;
            }
        }
    }

    pub fn generate(&mut self, max_particles: u32, seed: u64) -> u32 {
        let mut r = Rng::new(seed);
        let mut sum = 0.0f64;
        let mut mx = 0.0f32;
        self.hemi_power = [0.0; 2];
        for (i, &v) in self.grid.iter().enumerate() {
            mx = mx.max(v);
            if v >= 3.0 {
                sum += (v - 2.0) as f64;
                self.hemi_power[if i / W >= 90 { 0 } else { 1 }] += v;
            }
        }
        self.max = mx;
        self.parts.clear();
        if sum <= 0.0 {
            return 0;
        }
        let scale = max_particles as f64 / sum;
        let norm = mx.max(20.0);
        for y in 0..H {
            for x in 0..W {
                let v = self.grid[y * W + x];
                if v < 3.0 {
                    continue;
                }
                let want = (v - 2.0) as f64 * scale;
                let mut k = want.floor() as u32;
                if r.f() < want.fract() {
                    k += 1;
                }
                let inten = (v / norm).clamp(0.12, 1.0);
                for _ in 0..k.min(80) {
                    let lat = y as f64 - 90.0 + r.range(-0.5, 0.5);
                    let lon = x as f64 + r.range(-0.5, 0.5);
                    let af = r.f().powf(1.7);
                    let alt = 100.0 + 200.0 * af;
                    let p = geo::latlon_to_scene(lat, lon, 1.0 + alt / geo::RE_KM);
                    self.parts.extend_from_slice(&[p[0], p[1], p[2], inten, af as f32]);
                }
            }
        }
        (self.parts.len() / STRIDE) as u32
    }
}
