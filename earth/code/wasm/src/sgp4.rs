//! SGP4 orbit propagator (Vallado/Spacetrack Report #3 revision, WGS-72 constants).
//! Near-earth branch in full; objects with period >= 225 min use the simplified-drag
//! branch without the lunisolar SDP4 terms (adequate for visualisation, not for conjunctions).

use core::f64::consts::PI;

const MU: f64 = 398600.8;
const RE: f64 = 6378.135;
const J2: f64 = 0.001082616;
const J3: f64 = -0.00000253881;
const J4: f64 = -0.00000165597;
const J3OJ2: f64 = J3 / J2;
const X2O3: f64 = 2.0 / 3.0;
const TWO_PI: f64 = 2.0 * PI;

#[inline]
fn xke() -> f64 {
    60.0 / (RE * RE * RE / MU).sqrt()
}

#[derive(Clone, Copy, Default)]
pub struct Sat {
    pub epoch_jd: f64,
    pub bstar: f64,
    pub ecco: f64,
    pub inclo: f64,
    pub nodeo: f64,
    pub argpo: f64,
    pub mo: f64,
    pub no: f64, // un-Kozai'd mean motion, rad/min
    pub a: f64,  // semi-major axis, Earth radii
    isimp: bool,
    aycof: f64,
    con41: f64,
    cc1: f64,
    cc4: f64,
    cc5: f64,
    d2: f64,
    d3: f64,
    d4: f64,
    delmo: f64,
    eta: f64,
    argpdot: f64,
    omgcof: f64,
    sinmao: f64,
    t2cof: f64,
    t3cof: f64,
    t4cof: f64,
    t5cof: f64,
    x1mth2: f64,
    x7thm1: f64,
    mdot: f64,
    nodedot: f64,
    xlcof: f64,
    xmcof: f64,
    nodecf: f64,
}

impl Sat {
    /// `no_kozai` in rad/min, angles in radians.
    pub fn new(epoch_jd: f64, no_kozai: f64, ecco: f64, inclo: f64, nodeo: f64, argpo: f64, mo: f64, bstar: f64) -> Option<Sat> {
        if !(no_kozai > 0.0) || !(0.0..1.0).contains(&ecco) {
            return None;
        }
        let xke = xke();
        let mut s = Sat { epoch_jd, bstar, ecco, inclo, nodeo, argpo, mo, ..Default::default() };

        // ---- initl
        let ak = (xke / no_kozai).powf(X2O3);
        let cosio = inclo.cos();
        let cosio2 = cosio * cosio;
        let eccsq = ecco * ecco;
        let omeosq = 1.0 - eccsq;
        let rteosq = omeosq.sqrt();
        let d1 = 0.75 * J2 * (3.0 * cosio2 - 1.0) / (rteosq * omeosq);
        let mut del = d1 / (ak * ak);
        let adel = ak * (1.0 - del * del - del * (1.0 / 3.0 + 134.0 * del * del / 81.0));
        del = d1 / (adel * adel);
        let no = no_kozai / (1.0 + del);
        let ao = (xke / no).powf(X2O3);
        let sinio = inclo.sin();
        let po = ao * omeosq;
        let con42 = 1.0 - 5.0 * cosio2;
        let con41 = -con42 - cosio2 - cosio2;
        let posq = po * po;
        let rp = ao * (1.0 - ecco);
        s.no = no;
        s.a = ao;
        s.con41 = con41;

        // ---- sgp4init
        let ss = 78.0 / RE + 1.0;
        let qzms2t = ((120.0 - 78.0) / RE).powi(4);
        let deep = TWO_PI / no >= 225.0;
        s.isimp = rp < 220.0 / RE + 1.0 || deep;
        let mut sfour = ss;
        let mut qzms24 = qzms2t;
        let perige = (rp - 1.0) * RE;
        if perige < 156.0 {
            sfour = perige - 78.0;
            if perige < 98.0 {
                sfour = 20.0;
            }
            qzms24 = ((120.0 - sfour) / RE).powi(4);
            sfour = sfour / RE + 1.0;
        }
        let pinvsq = 1.0 / posq;
        let tsi = 1.0 / (ao - sfour);
        let eta = ao * ecco * tsi;
        let etasq = eta * eta;
        let eeta = ecco * eta;
        let psisq = (1.0 - etasq).abs();
        let coef = qzms24 * tsi.powi(4);
        let coef1 = coef / psisq.powf(3.5);
        let cc2 = coef1 * no * (ao * (1.0 + 1.5 * etasq + eeta * (4.0 + etasq))
            + 0.375 * J2 * tsi / psisq * con41 * (8.0 + 3.0 * etasq * (8.0 + etasq)));
        let cc1 = bstar * cc2;
        let cc3 = if ecco > 1.0e-4 { -2.0 * coef * tsi * J3OJ2 * no * sinio / ecco } else { 0.0 };
        let x1mth2 = 1.0 - cosio2;
        let cc4 = 2.0 * no * coef1 * ao * omeosq
            * (eta * (2.0 + 0.5 * etasq) + ecco * (0.5 + 2.0 * etasq)
                - J2 * tsi / (ao * psisq)
                    * (-3.0 * con41 * (1.0 - 2.0 * eeta + etasq * (1.5 - 0.5 * eeta))
                        + 0.75 * x1mth2 * (2.0 * etasq - eeta * (1.0 + etasq)) * (2.0 * argpo).cos()));
        let cc5 = 2.0 * coef1 * ao * omeosq * (1.0 + 2.75 * (etasq + eeta) + eeta * etasq);
        let cosio4 = cosio2 * cosio2;
        let temp1 = 1.5 * J2 * pinvsq * no;
        let temp2 = 0.5 * temp1 * J2 * pinvsq;
        let temp3 = -0.46875 * J4 * pinvsq * pinvsq * no;
        s.mdot = no + 0.5 * temp1 * rteosq * con41 + 0.0625 * temp2 * rteosq * (13.0 - 78.0 * cosio2 + 137.0 * cosio4);
        s.argpdot = -0.5 * temp1 * con42 + 0.0625 * temp2 * (7.0 - 114.0 * cosio2 + 395.0 * cosio4)
            + temp3 * (3.0 - 36.0 * cosio2 + 49.0 * cosio4);
        let xhdot1 = -temp1 * cosio;
        s.nodedot = xhdot1 + (0.5 * temp2 * (4.0 - 19.0 * cosio2) + 2.0 * temp3 * (3.0 - 7.0 * cosio2)) * cosio;
        s.omgcof = bstar * cc3 * argpo.cos();
        s.xmcof = if ecco > 1.0e-4 { -X2O3 * coef * bstar / eeta } else { 0.0 };
        s.nodecf = 3.5 * omeosq * xhdot1 * cc1;
        s.t2cof = 1.5 * cc1;
        let den = if (cosio + 1.0).abs() > 1.5e-12 { 1.0 + cosio } else { 1.5e-12 };
        s.xlcof = -0.25 * J3OJ2 * sinio * (3.0 + 5.0 * cosio) / den;
        s.aycof = -0.5 * J3OJ2 * sinio;
        s.delmo = (1.0 + eta * mo.cos()).powi(3);
        s.sinmao = mo.sin();
        s.x7thm1 = 7.0 * cosio2 - 1.0;
        s.eta = eta;
        s.cc1 = cc1;
        s.cc4 = cc4;
        s.cc5 = cc5;
        s.x1mth2 = x1mth2;
        if !s.isimp {
            let cc1sq = cc1 * cc1;
            s.d2 = 4.0 * ao * tsi * cc1sq;
            let temp = s.d2 * tsi * cc1 / 3.0;
            s.d3 = (17.0 * ao + sfour) * temp;
            s.d4 = 0.5 * temp * ao * tsi * (221.0 * ao + 31.0 * sfour) * cc1;
            s.t3cof = s.d2 + 2.0 * cc1sq;
            s.t4cof = 0.25 * (3.0 * s.d3 + cc1 * (12.0 * s.d2 + 10.0 * cc1sq));
            s.t5cof = 0.2 * (3.0 * s.d4 + 12.0 * cc1 * s.d3 + 6.0 * s.d2 * s.d2 + 15.0 * cc1sq * (2.0 * s.d2 + cc1sq));
        }
        Some(s)
    }

    pub fn period_min(&self) -> f64 {
        TWO_PI / self.no
    }

    /// Position (km) and velocity (km/s) in TEME at `t` minutes since epoch.
    pub fn propagate(&self, t: f64) -> Option<([f64; 3], [f64; 3])> {
        let xke = xke();
        let xmdf = self.mo + self.mdot * t;
        let argpdf = self.argpo + self.argpdot * t;
        let nodedf = self.nodeo + self.nodedot * t;
        let mut argpm = argpdf;
        let mut mm = xmdf;
        let t2 = t * t;
        let mut nodem = nodedf + self.nodecf * t2;
        let mut tempa = 1.0 - self.cc1 * t;
        let mut tempe = self.bstar * self.cc4 * t;
        let mut templ = self.t2cof * t2;
        if !self.isimp {
            let delomg = self.omgcof * t;
            let delm = self.xmcof * ((1.0 + self.eta * xmdf.cos()).powi(3) - self.delmo);
            let temp = delomg + delm;
            mm = xmdf + temp;
            argpm = argpdf - temp;
            let t3 = t2 * t;
            let t4 = t3 * t;
            tempa = tempa - self.d2 * t2 - self.d3 * t3 - self.d4 * t4;
            tempe += self.bstar * self.cc5 * (mm.sin() - self.sinmao);
            templ += self.t3cof * t3 + t4 * (self.t4cof + t * self.t5cof);
        }
        let nm0 = self.no;
        let mut em = self.ecco;
        let inclm = self.inclo;
        let am = (xke / nm0).powf(X2O3) * tempa * tempa;
        let nm = xke / am.powf(1.5);
        em -= tempe;
        if !(-0.001..1.0).contains(&em) || am < 0.95 {
            return None;
        }
        if em < 1.0e-6 {
            em = 1.0e-6;
        }
        mm += self.no * templ;
        let mut xlm = mm + argpm + nodem;
        nodem %= TWO_PI;
        argpm %= TWO_PI;
        xlm %= TWO_PI;
        mm = (xlm - argpm - nodem) % TWO_PI;
        let (sinip, cosip) = inclm.sin_cos();

        let axnl = em * argpm.cos();
        let mut temp = 1.0 / (am * (1.0 - em * em));
        let aynl = em * argpm.sin() + temp * self.aycof;
        let xl = mm + argpm + nodem + temp * self.xlcof * axnl;

        let u = (xl - nodem) % TWO_PI;
        let mut eo1 = u;
        let mut tem5: f64 = 9999.9;
        let mut ktr = 1;
        let (mut sineo1, mut coseo1) = (0.0, 0.0);
        while tem5.abs() >= 1.0e-12 && ktr <= 10 {
            let sc = eo1.sin_cos();
            sineo1 = sc.0;
            coseo1 = sc.1;
            tem5 = 1.0 - coseo1 * axnl - sineo1 * aynl;
            tem5 = (u - aynl * coseo1 + axnl * sineo1 - eo1) / tem5;
            if tem5.abs() >= 0.95 {
                tem5 = 0.95f64.copysign(tem5);
            }
            eo1 += tem5;
            ktr += 1;
        }

        let ecose = axnl * coseo1 + aynl * sineo1;
        let esine = axnl * sineo1 - aynl * coseo1;
        let el2 = axnl * axnl + aynl * aynl;
        let pl = am * (1.0 - el2);
        if pl < 0.0 {
            return None;
        }
        let rl = am * (1.0 - ecose);
        let rdotl = am.sqrt() * esine / rl;
        let rvdotl = pl.sqrt() / rl;
        let betal = (1.0 - el2).sqrt();
        temp = esine / (1.0 + betal);
        let sinu = am / rl * (sineo1 - aynl - axnl * temp);
        let cosu = am / rl * (coseo1 - axnl + aynl * temp);
        let mut su = sinu.atan2(cosu);
        let sin2u = (cosu + cosu) * sinu;
        let cos2u = 1.0 - 2.0 * sinu * sinu;
        temp = 1.0 / pl;
        let temp1 = 0.5 * J2 * temp;
        let temp2 = temp1 * temp;

        let mrt = rl * (1.0 - 1.5 * temp2 * betal * self.con41) + 0.5 * temp1 * self.x1mth2 * cos2u;
        su -= 0.25 * temp2 * self.x7thm1 * sin2u;
        let xnode = nodem + 1.5 * temp2 * cosip * sin2u;
        let xinc = inclm + 1.5 * temp2 * cosip * sinip * cos2u;
        let mvt = rdotl - nm * temp1 * self.x1mth2 * sin2u / xke;
        let rvdot = rvdotl + nm * temp1 * (self.x1mth2 * cos2u + 1.5 * self.con41) / xke;

        let (sinsu, cossu) = su.sin_cos();
        let (snod, cnod) = xnode.sin_cos();
        let (sini, cosi) = xinc.sin_cos();
        let xmx = -snod * cosi;
        let xmy = cnod * cosi;
        let ux = xmx * sinsu + cnod * cossu;
        let uy = xmy * sinsu + snod * cossu;
        let uz = sini * sinsu;
        let vx = xmx * cossu - cnod * sinsu;
        let vy = xmy * cossu - snod * sinsu;
        let vz = sini * cossu;
        if mrt < 1.0 {
            return None; // decayed
        }
        let vkms = RE * xke / 60.0;
        let r = [mrt * ux * RE, mrt * uy * RE, mrt * uz * RE];
        let v = [
            (mvt * ux + rvdot * vx) * vkms,
            (mvt * uy + rvdot * vy) * vkms,
            (mvt * uz + rvdot * vz) * vkms,
        ];
        Some((r, v))
    }
}

/// Julian date of 0h on January 0 of `year` (i.e. Dec 31 of the previous year).
pub fn jd_jan0(year: i32) -> f64 {
    let y = year as f64;
    367.0 * y - (7.0 * y / 4.0).floor() + 30.0 + 1721013.5
}

/// Kozai mean motion (rad/min) for a circular orbit at `alt_km`.
pub fn mean_motion_for_alt(alt_km: f64) -> f64 {
    let a = RE + alt_km;
    (MU / (a * a * a)).sqrt() * 60.0
}
