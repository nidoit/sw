//! Catalogue of orbiting objects: TLE parsing, synthetic populations, batched propagation
//! and screen-space picking. Positions are written to an interleaved instance buffer
//! (8 × f32 per object: x, y, z, t | vx, vy, vz, category) consumed directly by WebGL.

use crate::geo::{self, DEG, OMEGA_E, RE_KM};
use crate::rng::Rng;
use crate::scan;
use crate::sgp4::{self, Sat};
use core::f64::consts::PI;

pub const CAT_LEO: u8 = 0;
pub const CAT_MEO: u8 = 1;
pub const CAT_GEO: u8 = 2;
pub const CAT_SAT: u8 = 3;
pub const CAT_ISS: u8 = 4;
pub const CAT_DEAD: u8 = 9;
pub const STRIDE: usize = 8;

pub struct Obj {
    pub sat: Sat,
    pub cat: u8,
    pub synthetic: bool,
    pub norad: u32,
    name: (u32, u32),
}

#[derive(Default)]
pub struct Catalog {
    pub objs: Vec<Obj>,
    names: Vec<u8>,
    pub inst: Vec<f32>,
    cursor: usize,
}

/// Debris class from mean altitude.
pub fn classify(a_er: f64) -> u8 {
    let alt = (a_er - 1.0) * RE_KM;
    if alt < 2000.0 {
        CAT_LEO
    } else if (alt - 35786.0).abs() < 1000.0 {
        CAT_GEO
    } else {
        CAT_MEO
    }
}

fn is_debris_name(name: &[u8]) -> bool {
    let up: Vec<u8> = name.iter().map(|c| c.to_ascii_uppercase()).collect();
    scan::find(&up, b" DEB", 0).is_some() || scan::find(&up, b"R/B", 0).is_some() || scan::find(&up, b"DEBRIS", 0).is_some()
}

fn trim(mut s: &[u8]) -> &[u8] {
    while let [b' ' | b'\t' | b'\r', rest @ ..] = s {
        s = rest;
    }
    while let [rest @ .., b' ' | b'\t' | b'\r'] = s {
        s = rest;
    }
    s
}

fn alpha5(f: &[u8]) -> u32 {
    let f = trim(f);
    if f.is_empty() {
        return 0;
    }
    let lead = f[0].to_ascii_uppercase();
    let base = if lead.is_ascii_alphabetic() {
        // Alpha-5: A=10 … skipping I and O
        let mut v = (lead - b'A') as u32 + 10;
        if lead > b'I' {
            v -= 1;
        }
        if lead > b'O' {
            v -= 1;
        }
        v * 10000
    } else {
        0
    };
    let digits = if lead.is_ascii_alphabetic() { &f[1..] } else { f };
    base + digits.iter().filter(|c| c.is_ascii_digit()).fold(0u32, |a, c| a * 10 + (c - b'0') as u32)
}

impl Catalog {
    pub fn clear(&mut self, which: u32) {
        match which {
            1 => self.objs.retain(|o| o.synthetic),
            2 => self.objs.retain(|o| !o.synthetic),
            _ => {
                self.objs.clear();
                self.names.clear();
            }
        }
        self.cursor = 0;
        self.inst.resize(self.objs.len() * STRIDE, 0.0);
    }

    pub fn name(&self, i: usize) -> &[u8] {
        match self.objs.get(i) {
            Some(o) => &self.names[o.name.0 as usize..(o.name.0 + o.name.1) as usize],
            None => &[],
        }
    }

    fn push(&mut self, sat: Sat, cat: u8, synthetic: bool, norad: u32, name: &[u8]) {
        let off = self.names.len() as u32;
        self.names.extend_from_slice(name);
        self.objs.push(Obj { sat, cat, synthetic, norad, name: (off, name.len() as u32) });
        self.inst.extend_from_slice(&[0.0; STRIDE]);
        let i = self.objs.len() - 1;
        self.inst[i * STRIDE + 7] = CAT_DEAD as f32;
    }

    /// Parses 2LE/3LE text. kind: 0 = all debris, 1 = all active payloads, 2 = decide by name.
    pub fn load_tle(&mut self, text: &[u8], kind: u32) -> u32 {
        let lines: Vec<&[u8]> = text.split(|&c| c == b'\n').map(trim).collect();
        let mut added = 0;
        let mut i = 0;
        while i + 1 < lines.len() {
            let (l1, l2) = (lines[i], lines[i + 1]);
            if l1.len() >= 64 && l2.len() >= 63 && l1.starts_with(b"1 ") && l2.starts_with(b"2 ") {
                let mut name: &[u8] = if i > 0 { lines[i - 1] } else { b"" };
                if name.starts_with(b"1 ") || name.starts_with(b"2 ") {
                    name = b"";
                }
                if name.starts_with(b"0 ") {
                    name = &name[2..];
                }
                if let Some((sat, norad)) = parse_tle(l1, l2) {
                    let cat = if norad == 25544 {
                        CAT_ISS
                    } else {
                        let debris = match kind {
                            0 => true,
                            1 => false,
                            _ => is_debris_name(name),
                        };
                        if debris { classify(sat.a) } else { CAT_SAT }
                    };
                    let nm: Vec<u8> = if name.is_empty() { format!("NORAD {norad}").into_bytes() } else { name.to_vec() };
                    self.push(sat, cat, false, norad, &nm);
                    added += 1;
                }
                i += 2;
            } else {
                i += 1;
            }
        }
        added
    }

    /// Statistically plausible debris cloud (shell altitudes / inclinations after ESA MASTER trends).
    pub fn synth_debris(&mut self, count: u32, seed: u64, jd: f64) -> u32 {
        let mut r = Rng::new(seed);
        let mut added = 0;
        for k in 0..count {
            let pick = r.f();
            let (alt_p, alt_a, inc): (f64, f64, f64);
            if pick < 0.72 {
                // LEO
                let alt = match r.f() {
                    x if x < 0.45 => r.gauss(820.0, 110.0),
                    x if x < 0.65 => r.gauss(550.0, 50.0),
                    x if x < 0.75 => r.gauss(1450.0, 80.0),
                    _ => r.range(350.0, 1990.0),
                }
                .clamp(300.0, 1990.0);
                let e_span = r.f().powi(3) * 0.03;
                let dalt = e_span * (RE_KM + alt);
                alt_p = (alt - dalt).max(250.0);
                alt_a = alt + (alt - alt_p);
                inc = match r.f() {
                    x if x < 0.40 => r.gauss(98.6, 0.8),
                    x if x < 0.55 => r.gauss(74.0, 0.5),
                    x if x < 0.65 => r.gauss(82.5, 0.4),
                    x if x < 0.72 => r.gauss(86.4, 0.3),
                    x if x < 0.82 => r.gauss(52.5, 1.5),
                    _ => r.range(0.0, 110.0),
                };
            } else if pick < 0.85 {
                // MEO: GNSS shells, GTO stages, Molniya
                match r.f() {
                    x if x < 0.5 => {
                        let alt = match r.f() {
                            y if y < 0.33 => r.gauss(20180.0, 250.0),
                            y if y < 0.66 => r.gauss(19130.0, 250.0),
                            _ => r.gauss(23222.0, 300.0),
                        };
                        alt_p = alt - r.f() * 300.0;
                        alt_a = alt + r.f() * 300.0;
                        inc = r.gauss(58.0, 4.0);
                    }
                    x if x < 0.85 => {
                        alt_p = r.range(250.0, 700.0);
                        alt_a = r.gauss(35500.0, 800.0);
                        inc = r.range(5.0, 29.0);
                    }
                    _ => {
                        alt_p = r.range(500.0, 1500.0);
                        alt_a = r.gauss(39700.0, 500.0);
                        inc = r.gauss(63.4, 0.5);
                    }
                }
            } else {
                // GEO belt incl. graveyard orbits
                let alt = 35786.0 + if r.f() < 0.25 { r.range(250.0, 600.0) } else { r.gauss(0.0, 120.0) };
                alt_p = alt - r.f() * 60.0;
                alt_a = alt + r.f() * 60.0;
                inc = r.f().powi(2) * 15.0;
            }
            let a = RE_KM + (alt_p + alt_a) * 0.5;
            let e = ((alt_a - alt_p) / (2.0 * a)).clamp(0.0, 0.95);
            let n = sgp4::mean_motion_for_alt(a - RE_KM);
            let bstar = r.range(1e-6, 4e-5) * if alt_p < 600.0 { 0.2 } else { 1.0 };
            let sat = Sat::new(
                jd,
                n,
                e,
                inc.abs().min(179.0) * DEG,
                r.range(0.0, 2.0 * PI),
                r.range(0.0, 2.0 * PI),
                r.range(0.0, 2.0 * PI),
                bstar,
            );
            if let Some(sat) = sat {
                let cat = classify(sat.a);
                let name = format!("SIM DEB {:05}", k + 1);
                self.push(sat, cat, true, 900000 + k, name.as_bytes());
                added += 1;
            }
        }
        added
    }

    /// Synthetic active fleet (Walker-like shells of major constellations) used when live TLEs are unavailable.
    pub fn synth_active(&mut self, seed: u64, jd: f64) -> u32 {
        let mut r = Rng::new(seed ^ 0xA5A5);
        // (label, alt km, inc deg, planes, per plane)
        let shells: [(&str, f64, f64, u32, u32); 13] = [
            ("STARLINK", 550.0, 53.0, 72, 22),
            ("STARLINK", 540.0, 53.2, 72, 22),
            ("STARLINK", 570.0, 70.0, 36, 20),
            ("STARLINK", 560.0, 97.6, 6, 58),
            ("ONEWEB", 1200.0, 87.9, 12, 49),
            ("IRIDIUM", 780.0, 86.4, 6, 11),
            ("GLOBALSTAR", 1414.0, 52.0, 8, 6),
            ("GPS", 20180.0, 55.0, 6, 5),
            ("GALILEO", 23222.0, 56.0, 3, 8),
            ("GLONASS", 19130.0, 64.8, 3, 8),
            ("BEIDOU", 21528.0, 55.0, 3, 8),
            ("O3B", 8062.0, 0.05, 1, 20),
            ("GEO-COMSAT", 35786.0, 0.05, 1, 420),
        ];
        let mut added = 0;
        let mut serial = 0u32;
        for (label, alt, inc, planes, per) in shells {
            for p in 0..planes {
                let raan = 2.0 * PI * p as f64 / planes as f64;
                for s in 0..per {
                    let m = 2.0 * PI * (s as f64 + 0.5 * (p % 2) as f64) / per as f64 + r.gauss(0.0, 0.01);
                    let alt_j = alt + r.gauss(0.0, if alt > 30000.0 { 5.0 } else { 2.0 });
                    let inc_j = if alt > 30000.0 { r.f() * 0.1 } else { inc + r.gauss(0.0, 0.02) };
                    if let Some(sat) = Sat::new(jd, sgp4::mean_motion_for_alt(alt_j), 1e-4, inc_j * DEG, raan, 0.0, m, 1e-6) {
                        serial += 1;
                        let name = format!("{label}-SIM-{:04}", s + p * per + 1);
                        self.push(sat, CAT_SAT, true, 800000 + serial, name.as_bytes());
                        added += 1;
                    }
                }
            }
        }
        for (name, alt, inc, cat) in [
            ("ISS (ZARYA) [SIM]", 418.0, 51.64, CAT_ISS),
            ("HST [SIM]", 530.0, 28.47, CAT_SAT),
            ("CSS (TIANHE) [SIM]", 385.0, 41.47, CAT_SAT),
        ] {
            if let Some(sat) = Sat::new(jd, sgp4::mean_motion_for_alt(alt), 5e-4, inc * DEG, r.range(0.0, 6.28), 0.0, r.range(0.0, 6.28), 3e-5) {
                serial += 1;
                self.push(sat, cat, true, 800000 + serial, name.as_bytes());
                added += 1;
            }
        }
        added
    }

    /// ECEF state (scene units, scene units/s) of object `i` at `jd`.
    pub fn state(&self, i: usize, jd: f64, g: f64) -> Option<([f64; 3], [f64; 3], [f64; 3], [f64; 3])> {
        let o = &self.objs[i];
        let tsince = (jd - o.sat.epoch_jd) * 1440.0;
        let (r, v) = o.sat.propagate(tsince)?;
        let re = geo::eci_to_ecef(r, g);
        let ve = geo::eci_to_ecef(v, g);
        // Earth-relative velocity: v_ecef = R·v − ω × r
        let ve = [ve[0] + OMEGA_E * re[1], ve[1] - OMEGA_E * re[0], ve[2]];
        Some((re, ve, r, v))
    }

    /// Propagates up to `budget` objects round-robin (all when budget == 0). Returns processed count.
    pub fn update(&mut self, jd: f64, t_sec: f32, budget: u32) -> u32 {
        let n = self.objs.len();
        if n == 0 {
            return 0;
        }
        let todo = if budget == 0 || budget as usize >= n { n } else { budget as usize };
        let g = geo::gmst(jd);
        for _ in 0..todo {
            let i = self.cursor;
            self.cursor = (self.cursor + 1) % n;
            let b = i * STRIDE;
            match self.state(i, jd, g) {
                Some((re, ve, _, _)) => {
                    let p = geo::ecef_to_scene(re);
                    let v = geo::ecef_to_scene(ve);
                    self.inst[b..b + STRIDE].copy_from_slice(&[
                        p[0] as f32, p[1] as f32, p[2] as f32, t_sec,
                        v[0] as f32, v[1] as f32, v[2] as f32, self.objs[i].cat as f32,
                    ]);
                }
                None => self.inst[b + 7] = CAT_DEAD as f32,
            }
        }
        todo as u32
    }

    pub fn counts(&self, out: &mut [f32]) {
        out[..6].fill(0.0);
        for (i, o) in self.objs.iter().enumerate() {
            if self.inst[i * STRIDE + 7] as u8 == CAT_DEAD {
                out[5] += 1.0;
            } else {
                out[o.cat.min(4) as usize] += 1.0;
            }
        }
    }

    /// Nearest visible object to the cursor. `vp` is a column-major view-projection matrix.
    /// mask bits: 1 LEO, 2 MEO, 4 GEO, 8 satellites (incl. ISS).
    #[allow(clippy::too_many_arguments)]
    pub fn pick(&self, vp: &[f32], cam: [f32; 3], ndc: [f32; 2], view: [f32; 2], thresh_px: f32, mask: u32, t: f32) -> i32 {
        let mut best = -1i32;
        let mut best_d = thresh_px * thresh_px;
        let c2 = cam[0] * cam[0] + cam[1] * cam[1] + cam[2] * cam[2];
        for (i, inst) in self.inst.chunks_exact(STRIDE).enumerate() {
            let cat = inst[7] as u8;
            let bit = match cat {
                CAT_LEO => 1,
                CAT_MEO => 2,
                CAT_GEO => 4,
                CAT_SAT | CAT_ISS => 8,
                _ => 0,
            };
            if mask & bit == 0 {
                continue;
            }
            let dt = t - inst[3];
            let p = [inst[0] + inst[4] * dt, inst[1] + inst[5] * dt, inst[2] + inst[6] * dt];
            let w = vp[3] * p[0] + vp[7] * p[1] + vp[11] * p[2] + vp[15];
            if w <= 0.0 {
                continue;
            }
            let x = (vp[0] * p[0] + vp[4] * p[1] + vp[8] * p[2] + vp[12]) / w;
            let y = (vp[1] * p[0] + vp[5] * p[1] + vp[9] * p[2] + vp[13]) / w;
            let dx = (x - ndc[0]) * 0.5 * view[0];
            let dy = (y - ndc[1]) * 0.5 * view[1];
            let d = dx * dx + dy * dy;
            if d >= best_d {
                continue;
            }
            // occluded by the Earth? ray cam→p against unit sphere
            let dir = [p[0] - cam[0], p[1] - cam[1], p[2] - cam[2]];
            let len2 = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
            let bq = cam[0] * dir[0] + cam[1] * dir[1] + cam[2] * dir[2];
            let disc = bq * bq - len2 * (c2 - 1.0);
            if disc > 0.0 {
                let s = (-bq - disc.sqrt()) / len2;
                if s > 0.0 && s < 1.0 {
                    continue;
                }
            }
            best_d = d;
            best = i as i32;
        }
        best
    }

    /// Writes [alt km, speed km/s, lat, lon, inc deg, period min, perigee km, apogee km, norad, cat, synthetic, ecc] to `out`.
    pub fn info(&self, i: usize, jd: f64, out: &mut [f32]) -> bool {
        if i >= self.objs.len() {
            return false;
        }
        let g = geo::gmst(jd);
        let Some((re, _, _, v)) = self.state(i, jd, g) else { return false };
        let o = &self.objs[i];
        let rr = (re[0] * re[0] + re[1] * re[1] + re[2] * re[2]).sqrt();
        let lat = (re[2] / rr).asin() / DEG;
        let lon = re[1].atan2(re[0]) / DEG;
        let a_km = o.sat.a * RE_KM;
        let vals = [
            (rr - RE_KM) as f32,
            (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() as f32,
            lat as f32,
            lon as f32,
            (o.sat.inclo / DEG) as f32,
            o.sat.period_min() as f32,
            (a_km * (1.0 - o.sat.ecco) - RE_KM) as f32,
            (a_km * (1.0 + o.sat.ecco) - RE_KM) as f32,
            o.norad as f32,
            o.cat as f32,
            o.synthetic as u8 as f32,
            o.sat.ecco as f32,
        ];
        out[..vals.len()].copy_from_slice(&vals);
        true
    }
}

/// Parses one TLE pair into an initialised SGP4 record.
pub fn parse_tle(l1: &[u8], l2: &[u8]) -> Option<(Sat, u32)> {
    let norad = alpha5(&l1[2..7]);
    let yy = scan::field(l1, 18, 20) as i32;
    let days = scan::field(l1, 20, 32);
    let bstar = scan::tle_exp(l1, 53, 61);
    let inc = scan::field(l2, 8, 16);
    let raan = scan::field(l2, 17, 25);
    let ecc_digits = trim(&l2[26..33.min(l2.len())]);
    let mut ecc = 0.0;
    let mut div = 1.0;
    for &c in ecc_digits {
        if c.is_ascii_digit() {
            ecc = ecc * 10.0 + (c - b'0') as f64;
            div *= 10.0;
        }
    }
    ecc /= div;
    let argp = scan::field(l2, 34, 42);
    let m = scan::field(l2, 43, 51);
    let n_revday = scan::field(l2, 52, 63);
    let year = if yy < 57 { 2000 + yy } else { 1900 + yy };
    let epoch = sgp4::jd_jan0(year) + days;
    let no = n_revday * 2.0 * PI / 1440.0;
    Sat::new(epoch, no, ecc, inc * DEG, raan * DEG, argp * DEG, m * DEG, bstar).map(|s| (s, norad))
}
