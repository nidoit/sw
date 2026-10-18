//! earth.wasm — compute core of the 3D Earth monitor.
//!
//! JS hands raw text (TLE / JSON) or pixels to WASM through `io_alloc`, and reads results
//! back as typed-array views over linear memory (`*_ptr` + length). JS keeps only UI state
//! and rendering; parsing, orbit propagation, interpolation and particle physics live here.
#![allow(static_mut_refs)]

pub mod air;
pub mod aurora;
pub mod climate;
pub mod geo;
pub mod objects;
pub mod rng;
pub mod scan;
pub mod sgp4;

use objects::Catalog;

struct State {
    io: Vec<u8>,
    scratch: [f32; 64],
    cat: Catalog,
    aurora: aurora::Aurora,
    air: air::Air,
    img: Vec<u8>,
    sst: Vec<u8>,
    fires: climate::Fires,
    ice: Vec<f32>,
    ice_info: [f32; 8],
    neos: climate::Neos,
}

static mut ST: Option<State> = None;

fn st() -> &'static mut State {
    unsafe {
        ST.get_or_insert_with(|| State {
            io: Vec::new(),
            scratch: [0.0; 64],
            cat: Catalog::default(),
            aurora: aurora::Aurora::default(),
            air: air::Air::default(),
            img: Vec::new(),
            sst: Vec::new(),
            fires: climate::Fires::default(),
            ice: Vec::new(),
            ice_info: [0.0; 8],
            neos: climate::Neos::default(),
        })
    }
}

fn io(len: u32) -> &'static [u8] {
    let s = st();
    &s.io[..(len as usize).min(s.io.len())]
}

// ---------------------------------------------------------------- transfer
#[no_mangle]
pub extern "C" fn io_alloc(len: u32) -> *mut u8 {
    let s = st();
    s.io.clear();
    s.io.resize(len as usize, 0);
    s.io.as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn scratch_ptr() -> *mut f32 {
    st().scratch.as_mut_ptr()
}

// ---------------------------------------------------------------- orbits
#[no_mangle]
pub extern "C" fn obj_clear(which: u32) {
    st().cat.clear(which)
}

#[no_mangle]
pub extern "C" fn tle_load(len: u32, kind: u32) -> u32 {
    let text = io(len);
    st().cat.load_tle(text, kind)
}

#[no_mangle]
pub extern "C" fn synth_debris(count: u32, seed: u32, jd: f64) -> u32 {
    st().cat.synth_debris(count, seed as u64, jd)
}

#[no_mangle]
pub extern "C" fn synth_active(seed: u32, jd: f64) -> u32 {
    st().cat.synth_active(seed as u64, jd)
}

#[no_mangle]
pub extern "C" fn obj_count() -> u32 {
    st().cat.objs.len() as u32
}

#[no_mangle]
pub extern "C" fn inst_ptr() -> *const f32 {
    st().cat.inst.as_ptr()
}

#[no_mangle]
pub extern "C" fn update(jd: f64, t_sec: f32, budget: u32) -> u32 {
    st().cat.update(jd, t_sec, budget)
}

/// scratch[0..6] = LEO, MEO, GEO, satellites, ISS, decayed
#[no_mangle]
pub extern "C" fn obj_counts() {
    let s = st();
    s.cat.counts(&mut s.scratch);
}

/// Inputs in scratch: [0..16] view-projection matrix, [16..19] camera position.
#[no_mangle]
pub extern "C" fn pick(ndc_x: f32, ndc_y: f32, view_w: f32, view_h: f32, thresh_px: f32, mask: u32, t: f32) -> i32 {
    let s = st();
    let vp: [f32; 16] = s.scratch[..16].try_into().unwrap();
    let cam = [s.scratch[16], s.scratch[17], s.scratch[18]];
    s.cat.pick(&vp, cam, [ndc_x, ndc_y], [view_w, view_h], thresh_px, mask, t)
}

#[no_mangle]
pub extern "C" fn obj_info(i: u32, jd: f64) -> u32 {
    let s = st();
    s.cat.info(i as usize, jd, &mut s.scratch) as u32
}

#[no_mangle]
pub extern "C" fn name_ptr(i: u32) -> *const u8 {
    st().cat.name(i as usize).as_ptr()
}

#[no_mangle]
pub extern "C" fn name_len(i: u32) -> u32 {
    st().cat.name(i as usize).len() as u32
}

// ---------------------------------------------------------------- sun / time
/// scratch[0..3] = unit vector towards the Sun in scene coordinates.
#[no_mangle]
pub extern "C" fn sun_dir(jd: f64) {
    let v = geo::sun_ecef(jd);
    let s = st();
    s.scratch[0] = v[0] as f32;
    s.scratch[1] = v[2] as f32;
    s.scratch[2] = -v[1] as f32;
}

#[no_mangle]
pub extern "C" fn gmst(jd: f64) -> f64 {
    geo::gmst(jd)
}

// ---------------------------------------------------------------- aurora
#[no_mangle]
pub extern "C" fn aurora_parse(len: u32) -> u32 {
    let text = io(len);
    st().aurora.parse(text)
}

#[no_mangle]
pub extern "C" fn aurora_synth(kp: f64, jd: f64) {
    st().aurora.synth(kp, jd)
}

#[no_mangle]
pub extern "C" fn aurora_generate(max_particles: u32, seed: u32) -> u32 {
    st().aurora.generate(max_particles, seed as u64)
}

#[no_mangle]
pub extern "C" fn aurora_ptr() -> *const f32 {
    st().aurora.parts.as_ptr()
}

/// scratch[0..3] = max probability, northern power, southern power
#[no_mangle]
pub extern "C" fn aurora_stats() {
    let s = st();
    s.scratch[0] = s.aurora.max;
    s.scratch[1] = s.aurora.hemi_power[0];
    s.scratch[2] = s.aurora.hemi_power[1];
}

// ---------------------------------------------------------------- air quality
#[no_mangle]
pub extern "C" fn air_clear() {
    st().air.clear()
}

#[no_mangle]
pub extern "C" fn air_parse_openmeteo(len: u32) -> u32 {
    let text = io(len);
    st().air.parse_openmeteo(text)
}

#[no_mangle]
pub extern "C" fn air_parse_waqi(len: u32) -> u32 {
    let text = io(len);
    st().air.parse_waqi(text)
}

#[no_mangle]
pub extern "C" fn air_synth(seed: u32) -> u32 {
    st().air.synth(seed as u64)
}

#[no_mangle]
pub extern "C" fn air_build(w: u32, h: u32) -> *const u8 {
    st().air.build(w as usize, h as usize).as_ptr()
}

#[no_mangle]
pub extern "C" fn air_sample(lat: f64, lon: f64) -> f32 {
    st().air.sample(lat, lon)
}

/// scratch[0..3] = points, mean AQI, max AQI
#[no_mangle]
pub extern "C" fn air_stats() {
    let s = st();
    s.air.stats(&mut s.scratch);
}

#[no_mangle]
pub extern "C" fn air_particles_init(n: u32, seed: u32) {
    st().air.init_particles(n as usize, seed as u64)
}

#[no_mangle]
pub extern "C" fn air_step(dt: f64, speed: f64) -> *const f32 {
    let s = st();
    s.air.step(dt, speed);
    s.air.out.as_ptr()
}

// ---------------------------------------------------------------- imagery
/// Reprojects Web-Mercator RGBA (in the io buffer) to an equirectangular RGBA image.
#[no_mangle]
pub extern "C" fn img_merc_to_equirect(sw: u32, sh: u32, dw: u32, dh: u32) -> *const u8 {
    let s = st();
    let need = (sw * sh * 4) as usize;
    if s.io.len() < need {
        return core::ptr::null();
    }
    s.img = air::merc_to_equirect(&s.io[..need], sw as usize, sh as usize, dw as usize, dh as usize);
    s.img.as_ptr()
}

// ---------------------------------------------------------------- climate
#[no_mangle]
pub extern "C" fn sst_synth(w: u32, h: u32, enso: f64, seed: u32) -> *const u8 {
    let s = st();
    s.sst = climate::sst_synth(w as usize, h as usize, enso, seed as u64);
    s.sst.as_ptr()
}

#[no_mangle]
pub extern "C" fn fire_clear() {
    st().fires.clear()
}

#[no_mangle]
pub extern "C" fn fire_parse_csv(len: u32) -> u32 {
    let text = io(len);
    st().fires.parse_csv(text)
}

#[no_mangle]
pub extern "C" fn fire_synth(doy: f64, seed: u32) -> u32 {
    st().fires.synth(doy, seed as u64)
}

#[no_mangle]
pub extern "C" fn fire_build() -> u32 {
    st().fires.build()
}

#[no_mangle]
pub extern "C" fn fire_ptr() -> *const f32 {
    st().fires.out.as_ptr()
}

/// Builds sea-ice cap meshes. `north_ext`/`south_ext` ≤ 0 → trend model for (year, doy).
/// Mesh layout: [N current, N baseline, S current, S baseline], each (RINGS+1)·(SEG+1) xyz.
/// scratch[0..4] = N current, N baseline, S current, S baseline extents (10⁶ km²).
#[no_mangle]
pub extern "C" fn ice_build(north_ext: f64, south_ext: f64, year: f64, doy: f64) -> *const f32 {
    let s = st();
    let n_cur = if north_ext > 0.0 { north_ext } else { climate::ice_model(true, doy, year) };
    let s_cur = if south_ext > 0.0 { south_ext } else { climate::ice_model(false, doy, year) };
    let n_base = climate::ice_baseline(true, doy);
    let s_base = climate::ice_baseline(false, doy);
    s.ice.clear();
    climate::ice_cap(&mut s.ice, true, n_cur, 1.0025);
    climate::ice_cap(&mut s.ice, true, n_base, 1.003);
    climate::ice_cap(&mut s.ice, false, s_cur, 1.0025);
    climate::ice_cap(&mut s.ice, false, s_base, 1.003);
    s.ice_info[..4].copy_from_slice(&[n_cur as f32, n_base as f32, s_cur as f32, s_base as f32]);
    s.scratch[..4].copy_from_slice(&s.ice_info[..4]);
    s.ice.as_ptr()
}

/// Parses an NSIDC daily-extent CSV (io buffer). scratch[0..3] = extent, year, day-of-year; returns 1 on success.
#[no_mangle]
pub extern "C" fn ice_parse_csv(len: u32) -> u32 {
    let text = io(len);
    match climate::parse_ice_csv(text) {
        Some((e, y, d)) => {
            let s = st();
            s.scratch[..3].copy_from_slice(&[e as f32, y as f32, d as f32]);
            1
        }
        None => 0,
    }
}

#[no_mangle]
pub extern "C" fn ice_dims() -> u32 {
    ((climate::ICE_RINGS as u32) << 16) | climate::ICE_SEG as u32
}

#[no_mangle]
pub extern "C" fn neo_parse(len: u32) -> u32 {
    let text = io(len);
    st().neos.parse(text)
}

#[no_mangle]
pub extern "C" fn neo_build(now_ms: f64) -> u32 {
    let s = st();
    s.neos.build(now_ms);
    s.neos.list.len() as u32
}

#[no_mangle]
pub extern "C" fn neo_lines_ptr() -> *const f32 {
    st().neos.lines.as_ptr()
}

#[no_mangle]
pub extern "C" fn neo_now_ptr() -> *const f32 {
    st().neos.now.as_ptr()
}

#[no_mangle]
pub extern "C" fn neo_points() -> u32 {
    climate::NEO_PTS as u32
}

/// scratch[0..7] = miss km, miss LD, v km/s, diameter km, hazardous, CA day (unix), CA second-of-day
#[no_mangle]
pub extern "C" fn neo_info(i: u32) -> u32 {
    let s = st();
    s.neos.info(i as usize, &mut s.scratch) as u32
}

#[no_mangle]
pub extern "C" fn neo_name_ptr(i: u32) -> *const u8 {
    st().neos.list.get(i as usize).map_or(core::ptr::null(), |n| n.name.as_ptr())
}

#[no_mangle]
pub extern "C" fn neo_name_len(i: u32) -> u32 {
    st().neos.list.get(i as usize).map_or(0, |n| n.name.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgp4_vallado_00005() {
        let l1 = b"1 00005U 58002B   00179.78495062  .00000023  00000-0  28098-4 0  4753";
        let l2 = b"2 00005  34.2682 348.7242 1859667 331.7664  19.3264 10.82419157413667";
        let (sat, norad) = objects::parse_tle(l1, l2).unwrap();
        assert_eq!(norad, 5);
        let (r, v) = sat.propagate(0.0).unwrap();
        let exp_r = [7022.46529266, -1400.08296755, 0.03995155];
        let exp_v = [1.893841015, 6.405893759, 4.534807250];
        for k in 0..3 {
            assert!((r[k] - exp_r[k]).abs() < 1e-3, "r{k} {} vs {}", r[k], exp_r[k]);
            assert!((v[k] - exp_v[k]).abs() < 1e-6, "v{k}");
        }
        let (r, _) = sat.propagate(360.0).unwrap();
        let exp_r = [-7154.03120202, -3783.17682504, -3536.19412294];
        for k in 0..3 {
            assert!((r[k] - exp_r[k]).abs() < 1e-3, "r360 {k} {} vs {}", r[k], exp_r[k]);
        }
        let (r, _) = sat.propagate(4320.0).unwrap();
        let exp_r = [-9060.47373569, 4658.70952502, 813.68673153];
        for k in 0..3 {
            assert!((r[k] - exp_r[k]).abs() < 1e-2, "r4320 {k} {} vs {}", r[k], exp_r[k]);
        }
    }

    #[test]
    fn tle_3le_and_classify() {
        let txt = b"ISS (ZARYA)\n1 25544U 98067A   26290.50000000  .00016717  00000-0  30000-3 0  9993\n2 25544  51.6400 120.0000 0005000  90.0000 270.0000 15.50000000400000\nFENGYUN 1C DEB\n1 29228U 99025AK  26290.50000000  .00000500  00000-0  20000-3 0  9990\n2 29228  98.8000 200.0000 0050000  10.0000 350.0000 14.20000000 10000\n";
        let mut c = Catalog::default();
        assert_eq!(c.load_tle(txt, 2), 2);
        assert_eq!(c.objs[0].cat, objects::CAT_ISS);
        assert_eq!(c.objs[1].cat, objects::CAT_LEO);
        assert_eq!(c.name(1), b"FENGYUN 1C DEB");
        let jd = sgp4::jd_jan0(2026) + 290.6;
        assert_eq!(c.update(jd, 0.0, 0), 2);
        let mut out = [0f32; 16];
        assert!(c.info(0, jd, &mut out));
        assert!(out[0] > 350.0 && out[0] < 450.0, "ISS alt {}", out[0]);
        assert!(out[1] > 7.5 && out[1] < 7.8, "ISS speed {}", out[1]);
    }

    #[test]
    fn synth_population_classes() {
        let mut c = Catalog::default();
        let jd = 2461330.5;
        let n = c.synth_debris(30000, 7, jd);
        assert!(n > 29900);
        c.synth_active(1, jd);
        c.update(jd + 0.01, 0.0, 0);
        let mut cnt = [0f32; 8];
        c.counts(&mut cnt);
        assert!(cnt[0] > 18000.0 && cnt[1] > 2000.0 && cnt[2] > 3000.0 && cnt[3] > 5000.0, "{cnt:?}");
        assert!(cnt[5] < 50.0, "decayed {}", cnt[5]);
    }

    #[test]
    fn json_parsers() {
        let mut a = aurora::Aurora::default();
        let j = br#"{"Observation Time": "x", "coordinates": [[0, -90, 4], [10, 65, 37], [359, 70, 12]]}"#;
        assert_eq!(a.parse(j), 3);
        assert_eq!(a.grid[(65 + 90) * 360 + 10], 37.0);
        assert!(a.generate(1000, 1) > 100);

        let mut air = air::Air::default();
        let om = br#"[{"latitude":57.7,"longitude":12.0,"current_units":{"us_aqi":"USAQI"},"current":{"time":"t","interval":3600,"us_aqi":21,"pm2_5":4.1}},{"latitude":28.6,"longitude":77.2,"current_units":{"us_aqi":"USAQI"},"current":{"time":"t","interval":3600,"us_aqi":null}}]"#;
        assert_eq!(air.parse_openmeteo(om), 1);
        let wq = br#"{"status":"ok","data":[{"lat":28.6,"lon":77.2,"uid":1,"aqi":"182"},{"lat":1,"lon":2,"uid":2,"aqi":"-"}]}"#;
        assert_eq!(air.parse_waqi(wq), 1);
        air.build(90, 45);
        assert!(air.sample(28.6, 77.2) > 150.0);
    }

    #[test]
    fn climate_parsers() {
        let mut f = climate::Fires::default();
        let csv = b"latitude,longitude,bright_ti4,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_ti5,frp,daynight\n-8.1,-60.2,330.1,0.4,0.4,2026-10-17,0412,N,VIIRS,n,2.0NRT,290.0,12.5,N\n";
        assert_eq!(f.parse_csv(csv), 1);
        assert_eq!(f.build(), 1);
        let ice = b"Year, Month, Day, Extent, Missing, Source Data\nYYYY, MM, DD, 10^6 sq km, 10^6 sq km, Source data\n2026, 10, 16, 6.512, 0.000, x\n";
        let (e, y, d) = climate::parse_ice_csv(ice).unwrap();
        assert!((e - 6.512).abs() < 1e-9 && y == 2026.0 && d == 289.0);
        let mut n = climate::Neos::default();
        let j = br#"{"near_earth_objects":{"2026-10-18":[{"links":{},"id":"1","neo_reference_id":"1","name":"(2026 AB)","estimated_diameter":{"kilometers":{"estimated_diameter_min":0.1,"estimated_diameter_max":0.25}},"is_potentially_hazardous_asteroid":true,"close_approach_data":[{"epoch_date_close_approach":1792300000000,"relative_velocity":{"kilometers_per_second":"14.2"},"miss_distance":{"astronomical":"0.02","lunar":"7.7","kilometers":"2991957.1"}}]}]}}"#;
        assert_eq!(n.parse(j), 1);
        assert_eq!(n.list[0].name, b"(2026 AB)");
        assert!(n.list[0].hazardous && (n.list[0].v_kms - 14.2).abs() < 1e-9);
        n.build(1792300000000.0);
        assert_eq!(n.lines.len(), climate::NEO_PTS * 3);
    }
}
