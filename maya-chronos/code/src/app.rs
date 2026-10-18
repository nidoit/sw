//! Scene construction, per-frame update and DOM wiring.

use crate::anim::{Ease, Tween};
use crate::calendar::MayaDate;
use crate::draw2d::{self, Face, GearSpec};
use crate::gl::{Material, Mesh, Renderer};
use crate::glyphs::{self, GlyphKind, GlyphStore};
use crate::math::{self, Vec3};
use crate::mesh::{hex, MeshBuilder, Rgb};
use std::cell::RefCell;
use std::f64::consts::TAU;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::{
    CanvasRenderingContext2d, Document, Element, HtmlCanvasElement, WebGl2RenderingContext,
    WebGlTexture, Window,
};

// ---------- layout ----------

const H: f32 = 1.2; // gear thickness; tops are at y = 0
const TEX_SIZE: u32 = 2048;
const SEGMENTS: u32 = 192;
const RED: &str = "#a61e17";
const BLACK: &str = "#1a1410";

const GATE_Y: f32 = 0.55;
const GATE_X_IN: f32 = 2.2;
const GATE_X_OUT: f32 = 27.5;
const GATE_DIVIDER_X: f32 = 16.0; // between the date group and the time group
const FOG: [f32; 3] = [5.0 / 255.0, 5.0 / 255.0, 5.0 / 255.0];

fn gear_specs() -> Vec<GearSpec> {
    let g = |inner_r, outer_r, slots, face, ink, teeth| GearSpec { inner_r, outer_r, slots, face, ink, teeth };
    vec![
        g(3.0, 5.6, 13, Face::Numeral { offset: 1 }, RED, false), // Tzolk'in number 1..13
        g(5.8, 9.4, 20, Face::Glyph(GlyphKind::Tzolkin), RED, false), // Tzolk'in day sign
        g(9.6, 12.0, 20, Face::Numeral { offset: 0 }, RED, false), // Haab' day 0..19
        g(12.2, 15.6, 19, Face::Glyph(GlyphKind::Haab), RED, true), // Haab' month
        g(16.4, 19.6, 24, Face::Numeral { offset: 0 }, BLACK, false), // hours
        g(19.8, 23.2, 60, Face::Numeral { offset: 0 }, BLACK, false), // minutes
        g(23.4, 26.8, 60, Face::Numeral { offset: 0 }, BLACK, true), // seconds
    ]
}

/// (duration s, easing) for each gear's tick animation, same order as `gear_specs`.
const TICK_ANIM: [(f64, Ease); 7] = [
    (1.5, Ease::Power3InOut),
    (1.5, Ease::Power3InOut),
    (1.5, Ease::Power3InOut),
    (1.5, Ease::Power3InOut),
    (1.0, Ease::Power2InOut),
    (0.6, Ease::Power2InOut),
    (0.25, Ease::BackOut),
];

// ---------- scene objects ----------

struct Gear {
    spec: GearSpec,
    body: Mesh,
    top: Mesh,
    tex: WebGlTexture,
    /// Cumulative slot count advanced (keeps rotating the same way forever).
    ticks: i64,
    last: Option<i64>,
    rot: Tween,
}

struct Prop {
    mesh: Mesh,
    tex: Option<WebGlTexture>,
    mat: Material,
}

const STONE: Material = Material { metal: 0.05, shininess: 12.0, unlit: false };
const GOLD: Material = Material { metal: 0.9, shininess: 60.0, unlit: false };
const DARK_METAL: Material = Material { metal: 0.7, shininess: 30.0, unlit: false };
const BRONZE: Material = Material { metal: 0.6, shininess: 35.0, unlit: false };
const UNLIT: Material = Material { metal: 0.0, shininess: 1.0, unlit: true };

#[derive(Clone, Copy)]
struct View {
    pos: Vec3,
    target: Vec3,
}

fn view_for(detail: bool, aspect: f32) -> View {
    // Back the camera off on narrow (portrait) screens so the whole gate fits.
    let k = (1.4 / aspect).clamp(1.0, 2.6);
    if detail {
        let t = Vec3::new(15.0, 0.0, 0.0);
        View { pos: Vec3::new(15.0, 24.0 * k, 18.0 * k), target: t }
    } else {
        View { pos: Vec3::new(0.0, 54.0 * k.sqrt(), 44.0 * k.sqrt()), target: Vec3::new(0.0, 0.0, 0.0) }
    }
}

pub struct App {
    window: Window,
    doc: Document,
    canvas: HtmlCanvasElement,
    r: Renderer,
    gears: Vec<Gear>,
    props: Vec<Prop>,
    glyphs: Rc<RefCell<GlyphStore>>,
    glyphs_applied: [bool; 2],
    cam: [Tween; 6],
    detail: bool,
    ui_canvas: HtmlCanvasElement,
    ui_ctx: CanvasRenderingContext2d,
    last_second: i64,
    el: Readout,
    /// Indices into `props` of the DATE / TIME gate plaques.
    label_props: [usize; 2],
    /// Button text for [overview, detail] views; replaced by `set_labels`.
    button_text: [String; 2],
}

thread_local! {
    static APP: RefCell<Option<Rc<RefCell<App>>>> = const { RefCell::new(None) };
}

struct Readout {
    long_count: Option<Element>,
    tzolkin: Option<Element>,
    haab: Option<Element>,
    clock: Option<Element>,
    button_label: Option<Element>,
}

fn set_text(el: &Option<Element>, s: &str) {
    if let Some(e) = el {
        e.set_text_content(Some(s));
    }
}

impl App {
    fn new(window: Window) -> Result<Self, JsValue> {
        let doc = window.document().ok_or("no document")?;
        let canvas: HtmlCanvasElement = doc.get_element_by_id("gl").ok_or("#gl canvas missing")?.dyn_into()?;
        let gl: WebGl2RenderingContext = canvas
            .get_context("webgl2")?
            .ok_or("WebGL2 is not available in this browser")?
            .dyn_into()?;
        let r = Renderer::new(gl)?;

        let glyphs = glyphs::start_loading()?;

        // gears
        let mut gears = Vec::new();
        for spec in gear_specs() {
            let mut body = MeshBuilder::new();
            let side: Rgb = hex(0x2a2520);
            body.tube_wall(spec.outer_r, -H, 0.0, SEGMENTS, true, side);
            body.tube_wall(spec.inner_r, -H, 0.0, SEGMENTS, false, side);
            body.annulus_bottom(spec.inner_r, spec.outer_r, -H, SEGMENTS, side);
            if spec.teeth {
                let step = std::f32::consts::TAU / spec.slots as f32;
                let rm = spec.outer_r;
                let tang = (step * rm * 0.45).min(0.6);
                for i in 0..spec.slots {
                    let a = (i as f32 + 0.5) * step; // between slots, under the dividers
                    let p = Vec3::new(a.cos() * (rm + 0.22), -H * 0.5, -a.sin() * (rm + 0.22));
                    body.cuboid(p, Vec3::new(0.44, H * 0.8, tang), a, hex(0xe0d8c0));
                }
            }
            let mut top = MeshBuilder::new();
            top.annulus_top(spec.inner_r, spec.outer_r, 0.0, SEGMENTS, spec.outer_r, [1.0, 1.0, 1.0]);

            let face = draw2d::gear_face(&doc, &spec, &glyphs.borrow(), TEX_SIZE)?;
            let tex = r.canvas_texture(&face, None)?;
            gears.push(Gear {
                spec,
                body: r.upload(&body)?,
                top: r.upload(&top)?,
                tex,
                ticks: 0,
                last: None,
                rot: Tween::fixed(0.0),
            });
        }

        // static props
        let mut props = Vec::new();

        let mut plinth = MeshBuilder::new();
        plinth.cylinder(28.6, -1.8, -1.25, SEGMENTS, hex(0x15120f));
        props.push(Prop { mesh: r.upload(&plinth)?, tex: None, mat: STONE });

        let mut core = MeshBuilder::new();
        core.cylinder(2.8, -1.8, 0.3, 64, hex(0x5a4a35));
        props.push(Prop { mesh: r.upload(&core)?, tex: None, mat: BRONZE });

        let gold = hex(0xd4af37);
        let mut gate = MeshBuilder::new();
        let len = GATE_X_OUT - GATE_X_IN;
        let mid = (GATE_X_IN + GATE_X_OUT) / 2.0;
        for z in [-1.9, 1.9] {
            gate.cuboid(Vec3::new(mid, GATE_Y, z), Vec3::new(len, 0.4, 0.6), 0.0, gold);
        }
        for x in [GATE_X_IN, GATE_X_OUT] {
            gate.cuboid(Vec3::new(x, GATE_Y, 0.0), Vec3::new(0.7, 0.4, 4.4), 0.0, gold);
        }
        gate.pyramid_x(Vec3::new(GATE_X_OUT + 1.7, GATE_Y, 0.0), 0.55, 1.2, -1.0, gold);
        gate.pyramid_x(Vec3::new(GATE_X_IN - 1.5, GATE_Y, 0.0), 0.45, 1.0, 1.0, gold);
        gate.cylinder(1.1, 0.3, 0.6, 48, gold); // hub cap
        props.push(Prop { mesh: r.upload(&gate)?, tex: None, mat: GOLD });

        let mut divider = MeshBuilder::new();
        divider.cuboid(Vec3::new(GATE_DIVIDER_X, GATE_Y + 0.05, 0.0), Vec3::new(0.5, 0.5, 4.4), 0.0, hex(0x222222));
        props.push(Prop { mesh: r.upload(&divider)?, tex: None, mat: DARK_METAL });

        let mut label_props = [0; 2];
        for (k, (text, x)) in [
            ("DATE", (GATE_X_IN + GATE_DIVIDER_X) / 2.0),
            ("TIME", (GATE_DIVIDER_X + GATE_X_OUT) / 2.0),
        ]
        .into_iter()
        .enumerate()
        {
            let mut plane = MeshBuilder::new();
            plane.plane_xz(Vec3::new(x, 0.36, -2.95), 4.8, 1.2, [1.0, 1.0, 1.0]);
            let tex = r.canvas_texture(&draw2d::label(&doc, text)?, None)?;
            label_props[k] = props.len();
            props.push(Prop { mesh: r.upload(&plane)?, tex: Some(tex), mat: UNLIT });
        }

        // UI clock canvas (crisp on HiDPI)
        let ui_canvas: HtmlCanvasElement =
            doc.get_element_by_id("maya-ui-clock").ok_or("#maya-ui-clock missing")?.dyn_into()?;
        let dpr = window.device_pixel_ratio().clamp(1.0, 3.0);
        ui_canvas.set_width((120.0 * dpr) as u32);
        ui_canvas.set_height((240.0 * dpr) as u32);
        let ui_ctx: CanvasRenderingContext2d =
            ui_canvas.get_context("2d")?.ok_or("no 2d context")?.dyn_into()?;

        let el = Readout {
            long_count: doc.get_element_by_id("lc"),
            tzolkin: doc.get_element_by_id("tz"),
            haab: doc.get_element_by_id("haab"),
            clock: doc.get_element_by_id("clock"),
            button_label: doc.query_selector("#toggle-cam-btn span").ok().flatten(),
        };

        let (w, h) = (
            window.inner_width()?.as_f64().unwrap_or(1.0),
            window.inner_height()?.as_f64().unwrap_or(1.0),
        );
        let v = view_for(false, (w / h.max(1.0)) as f32);
        let cam = [
            Tween::fixed(v.pos.x as f64),
            Tween::fixed(v.pos.y as f64),
            Tween::fixed(v.pos.z as f64),
            Tween::fixed(v.target.x as f64),
            Tween::fixed(v.target.y as f64),
            Tween::fixed(v.target.z as f64),
        ];

        Ok(App {
            window,
            doc,
            canvas,
            r,
            gears,
            props,
            glyphs,
            glyphs_applied: [false; 2],
            cam,
            detail: false,
            ui_canvas,
            ui_ctx,
            last_second: i64::MIN,
            el,
            label_props,
            button_text: ["Read The Gate (Zoom In)".into(), "View Entire Mechanism (Zoom Out)".into()],
        })
    }

    fn aspect(&self) -> f32 {
        self.canvas.width() as f32 / self.canvas.height().max(1) as f32
    }

    fn toggle_view(&mut self, now: f64) {
        self.detail = !self.detail;
        set_text(&self.el.button_label, &self.button_text[self.detail as usize]);
        self.retarget_camera(now, 2.0);
    }

    fn set_labels(&mut self, date: &str, time: &str, zoom_in: &str, zoom_out: &str) -> Result<(), JsValue> {
        for (i, text) in self.label_props.into_iter().zip([date, time]) {
            let canvas = draw2d::label(&self.doc, text)?;
            let tex = self.props[i].tex.take();
            self.props[i].tex = Some(self.r.canvas_texture(&canvas, tex)?);
        }
        self.button_text = [zoom_in.into(), zoom_out.into()];
        set_text(&self.el.button_label, &self.button_text[self.detail as usize]);
        Ok(())
    }

    fn retarget_camera(&mut self, now: f64, duration: f64) {
        let v = view_for(self.detail, self.aspect());
        let vals = [v.pos.x, v.pos.y, v.pos.z, v.target.x, v.target.y, v.target.z];
        for (t, val) in self.cam.iter_mut().zip(vals) {
            t.retarget(val as f64, now, duration, Ease::Power3InOut);
        }
    }

    /// Keeps the drawing buffer matched to the CSS size × devicePixelRatio.
    fn resize(&mut self, now: f64) {
        let dpr = self.window.device_pixel_ratio().clamp(1.0, 2.0);
        let w = (self.canvas.client_width() as f64 * dpr).round().max(1.0) as u32;
        let h = (self.canvas.client_height() as f64 * dpr).round().max(1.0) as u32;
        if w != self.canvas.width() || h != self.canvas.height() {
            self.canvas.set_width(w);
            self.canvas.set_height(h);
            self.retarget_camera(now, 0.4);
        }
    }

    fn on_second(&mut self, date: &js_sys::Date, now: f64) -> Result<(), JsValue> {
        let (h, m, s) = (date.get_hours(), date.get_minutes(), date.get_seconds());
        let maya = MayaDate::from_gregorian(
            date.get_full_year() as i64,
            date.get_month() as i64 + 1,
            date.get_date() as i64,
        );
        let values: [i64; 7] = [
            maya.tzolkin_number as i64 - 1,
            maya.tzolkin_name as i64,
            maya.haab_day as i64,
            maya.haab_month as i64,
            h as i64,
            m as i64,
            s as i64,
        ];
        for (i, gear) in self.gears.iter_mut().enumerate() {
            let slots = gear.spec.slots as i64;
            let step = TAU / slots as f64;
            let v = values[i];
            match gear.last {
                None => {
                    gear.ticks = v;
                    gear.rot = Tween::fixed(-(gear.ticks as f64) * step);
                }
                Some(prev) if prev != v => {
                    gear.ticks += (v - prev).rem_euclid(slots);
                    let (dur, ease) = TICK_ANIM[i];
                    gear.rot.retarget(-(gear.ticks as f64) * step, now, dur, ease);
                }
                _ => {}
            }
            gear.last = Some(v);
        }

        let (w, hh) = (self.ui_canvas.width() as f64, self.ui_canvas.height() as f64);
        draw2d::ui_clock(&self.ui_ctx, w, hh, [h, m, s])?;
        set_text(&self.el.long_count, &maya.long_count_string());
        set_text(&self.el.tzolkin, &maya.tzolkin_string());
        set_text(&self.el.haab, &maya.haab_string());
        set_text(&self.el.clock, &format!("{h:02}:{m:02}:{s:02}"));
        Ok(())
    }

    /// Once all images of a glyph set have loaded or failed, redraw that gear's texture.
    fn apply_glyphs(&mut self) -> Result<(), JsValue> {
        for (slot, kind) in [(0usize, GlyphKind::Tzolkin), (1, GlyphKind::Haab)] {
            if self.glyphs_applied[slot] {
                continue;
            }
            let store = self.glyphs.borrow();
            let set = store.set(kind);
            if !set.settled {
                continue;
            }
            web_sys::console::log_1(&format!("{kind:?} glyphs: {}/{} loaded", set.loaded_count(), set.images.len()).into());
            for gear in self.gears.iter_mut() {
                if matches!(gear.spec.face, Face::Glyph(k) if k == kind) {
                    let face = draw2d::gear_face(&self.doc, &gear.spec, &store, TEX_SIZE)?;
                    gear.tex = self.r.canvas_texture(&face, Some(gear.tex.clone()))?;
                }
            }
            self.glyphs_applied[slot] = true;
        }
        Ok(())
    }

    fn frame(&mut self, now: f64) -> Result<(), JsValue> {
        self.resize(now);

        let date = js_sys::Date::new_0();
        let sec = (date.get_time() / 1000.0).floor() as i64;
        if sec != self.last_second {
            self.last_second = sec;
            self.on_second(&date, now)?;
        }
        self.apply_glyphs()?;

        let eye = Vec3::new(self.cam[0].value(now) as f32, self.cam[1].value(now) as f32, self.cam[2].value(now) as f32);
        let target = Vec3::new(self.cam[3].value(now) as f32, self.cam[4].value(now) as f32, self.cam[5].value(now) as f32);
        let proj = math::perspective(45f32.to_radians(), self.aspect(), 0.1, 400.0);
        let view = math::look_at(eye, target, Vec3::new(0.0, 1.0, 0.0));
        let vp = math::mul(&proj, &view);

        self.r.begin_frame(self.canvas.width() as i32, self.canvas.height() as i32, &vp, eye, FOG);
        let ident = math::identity();
        for p in &self.props {
            self.r.draw(&p.mesh, &ident, p.tex.as_ref(), p.mat);
        }
        for g in &self.gears {
            let model = math::rotation_y(g.rot.value(now) as f32);
            self.r.draw(&g.body, &model, None, STONE);
            self.r.draw(&g.top, &model, Some(&g.tex), STONE);
        }
        Ok(())
    }
}

pub fn run() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let app = Rc::new(RefCell::new(App::new(window.clone())?));
    APP.with(|a| *a.borrow_mut() = Some(app.clone()));

    // camera toggle button
    if let Some(btn) = app.borrow().doc.get_element_by_id("toggle-cam-btn") {
        let app_c = app.clone();
        let win = window.clone();
        let cb = Closure::<dyn FnMut()>::new(move || {
            let now = win.performance().map(|p| p.now() / 1000.0).unwrap_or(0.0);
            app_c.borrow_mut().toggle_view(now);
        });
        btn.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // requestAnimationFrame loop
    let raf: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
    let raf_c = raf.clone();
    let win = window.clone();
    *raf.borrow_mut() = Some(Closure::new(move |t_ms: f64| {
        if let Err(e) = app.borrow_mut().frame(t_ms / 1000.0) {
            web_sys::console::error_1(&e);
        }
        if let Some(cb) = raf_c.borrow().as_ref() {
            let _ = win.request_animation_frame(cb.as_ref().unchecked_ref());
        }
    }));
    if let Some(cb) = raf.borrow().as_ref() {
        window.request_animation_frame(cb.as_ref().unchecked_ref())?;
    }
    Ok(())
}

/// Relabels the gate plaques and the zoom button (called from the page's language switch).
pub fn set_labels(date: &str, time: &str, zoom_in: &str, zoom_out: &str) -> Result<(), JsValue> {
    APP.with(|a| match a.borrow().as_ref() {
        Some(app) => app.borrow_mut().set_labels(date, time, zoom_in, zoom_out),
        None => Err("app not started".into()),
    })
}
