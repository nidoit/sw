//! 2D canvas drawing: Maya bar-and-dot numerals, gear face textures, labels, UI clock.

use crate::calendar::{HAAB_NAMES, TZOLKIN_NAMES};
use crate::glyphs::{GlyphKind, GlyphStore};
use std::f64::consts::{PI, TAU};
use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, Document, HtmlCanvasElement, HtmlImageElement};

// ---------- numerals ----------
// Unit geometry (scale 1): dot radius 6, dot pitch 16, bar 48x10, bar pitch 14.
const DOT_R: f64 = 6.0;
const DOT_PITCH: f64 = 16.0;
const BAR_W: f64 = 48.0;
const BAR_H: f64 = 10.0;
const BAR_PITCH: f64 = 14.0;
const TIER_GAP: f64 = 12.0;
const SHELL_W: f64 = 44.0;
const SHELL_H: f64 = 26.0;

fn tier_size(n: u32) -> (f64, f64) {
    if n == 0 {
        return (SHELL_W, SHELL_H);
    }
    let bars = n / 5;
    let dots = n % 5;
    let mut h = 0.0;
    let mut w: f64 = 0.0;
    if bars > 0 {
        h += bars as f64 * BAR_PITCH - (BAR_PITCH - BAR_H);
        w = BAR_W;
    }
    if dots > 0 {
        h += DOT_R * 2.0 + if bars > 0 { 4.0 } else { 0.0 };
        w = w.max(dots as f64 * DOT_PITCH - (DOT_PITCH - DOT_R * 2.0));
    }
    (w, h)
}

/// Unscaled bounding box of a numeral (vigesimal, up to 399).
pub fn numeral_size(n: u32) -> (f64, f64) {
    if n < 20 {
        tier_size(n)
    } else {
        let (w1, h1) = tier_size(n / 20);
        let (w2, h2) = tier_size(n % 20);
        (w1.max(w2), h1 + TIER_GAP + h2)
    }
}

fn rounded_rect(ctx: &CanvasRenderingContext2d, x: f64, y: f64, w: f64, h: f64, r: f64) -> Result<(), JsValue> {
    let r = r.min(h / 2.0).min(w / 2.0);
    ctx.begin_path();
    ctx.move_to(x + r, y);
    ctx.line_to(x + w - r, y);
    ctx.arc(x + w - r, y + r, r, -PI / 2.0, 0.0)?;
    ctx.line_to(x + w, y + h - r);
    ctx.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0)?;
    ctx.line_to(x + r, y + h);
    ctx.arc(x + r, y + h - r, r, PI / 2.0, PI)?;
    ctx.line_to(x, y + r);
    ctx.arc(x + r, y + r, r, PI, PI * 1.5)?;
    ctx.close_path();
    Ok(())
}

/// Draws one tier with its top edge at `top` (scaled coordinates).
fn draw_tier(ctx: &CanvasRenderingContext2d, n: u32, cx: f64, top: f64, s: f64) -> Result<(), JsValue> {
    if n == 0 {
        // shell glyph
        let cy = top + SHELL_H * s / 2.0;
        ctx.set_line_width(3.5 * s);
        ctx.begin_path();
        ctx.ellipse(cx, cy, SHELL_W * s / 2.0, SHELL_H * s / 2.0 - 1.5 * s, 0.0, 0.0, TAU)?;
        ctx.stroke();
        ctx.begin_path();
        ctx.move_to(cx - 15.0 * s, cy);
        ctx.line_to(cx + 15.0 * s, cy);
        ctx.move_to(cx - 7.0 * s, cy - 8.0 * s);
        ctx.line_to(cx - 3.0 * s, cy + 8.0 * s);
        ctx.move_to(cx + 7.0 * s, cy - 8.0 * s);
        ctx.line_to(cx + 3.0 * s, cy + 8.0 * s);
        ctx.stroke();
        return Ok(());
    }
    let bars = n / 5;
    let dots = n % 5;
    let mut y = top;
    if dots > 0 {
        let row_w = dots as f64 * DOT_PITCH - (DOT_PITCH - DOT_R * 2.0);
        let x0 = cx - row_w * s / 2.0 + DOT_R * s;
        for i in 0..dots {
            ctx.begin_path();
            ctx.arc(x0 + i as f64 * DOT_PITCH * s, y + DOT_R * s, DOT_R * s, 0.0, TAU)?;
            ctx.fill();
        }
        y += (DOT_R * 2.0 + if bars > 0 { 4.0 } else { 0.0 }) * s;
    }
    for i in 0..bars {
        rounded_rect(ctx, cx - BAR_W * s / 2.0, y + i as f64 * BAR_PITCH * s, BAR_W * s, BAR_H * s, BAR_H * s / 2.0)?;
        ctx.fill();
    }
    Ok(())
}

/// Draws numeral `n` centred on (cx, cy) at scale `s`.
pub fn draw_numeral(ctx: &CanvasRenderingContext2d, n: u32, cx: f64, cy: f64, s: f64, color: &str) -> Result<(), JsValue> {
    ctx.set_fill_style_str(color);
    ctx.set_stroke_style_str(color);
    ctx.set_line_cap("round");
    let (_, h) = numeral_size(n);
    let top = cy - h * s / 2.0;
    if n < 20 {
        draw_tier(ctx, n, cx, top, s)
    } else {
        let (_, h1) = tier_size(n / 20);
        draw_tier(ctx, n / 20, cx, top, s)?;
        draw_tier(ctx, n % 20, cx, top + (h1 + TIER_GAP) * s, s)
    }
}

// ---------- canvas helpers ----------

pub fn make_canvas(doc: &Document, w: u32, h: u32) -> Result<(HtmlCanvasElement, CanvasRenderingContext2d), JsValue> {
    let canvas: HtmlCanvasElement = doc.create_element("canvas")?.dyn_into()?;
    canvas.set_width(w);
    canvas.set_height(h);
    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?
        .dyn_into()?;
    Ok((canvas, ctx))
}

// ---------- gear faces ----------

#[derive(Clone, Copy, Debug)]
pub enum Face {
    /// Bar-and-dot numerals; slot i shows `i + offset`.
    Numeral { offset: u32 },
    Glyph(GlyphKind),
}

#[derive(Clone, Copy, Debug)]
pub struct GearSpec {
    pub inner_r: f32,
    pub outer_r: f32,
    pub slots: u32,
    pub face: Face,
    pub ink: &'static str,
    pub teeth: bool,
}

/// Width (world units) of the gate window: glyphs are sized so they fit inside it.
pub const GATE_WINDOW: f64 = 3.2;

/// Texture for the top face of a gear. The disc of radius `outer_r` fills the canvas;
/// slot i sits at angle i/slots·2π (counter-clockwise from +X seen from above),
/// drawn upright as seen from the gate side (+Z camera looking toward −Z).
pub fn gear_face(doc: &Document, spec: &GearSpec, glyphs: &GlyphStore, size: u32) -> Result<HtmlCanvasElement, JsValue> {
    let (canvas, ctx) = make_canvas(doc, size, size)?;
    let c = size as f64 / 2.0;
    let k = size as f64 / (2.0 * spec.outer_r as f64); // px per world unit
    let ro = spec.outer_r as f64 * k;
    let ri = spec.inner_r as f64 * k;
    let rm = (ro + ri) / 2.0;

    // stone ring with a soft radial shading
    let grad = ctx.create_radial_gradient(c, c, ri, c, c, ro)?;
    grad.add_color_stop(0.0, "#d9d0b6")?;
    grad.add_color_stop(0.5, "#e6dec7")?;
    grad.add_color_stop(1.0, "#cfc5a8")?;
    ctx.set_fill_style_canvas_gradient(&grad);
    ctx.begin_path();
    ctx.arc(c, c, ro, 0.0, TAU)?;
    ctx.arc_with_anticlockwise(c, c, ri, 0.0, TAU, true)?;
    ctx.fill();

    // speckle for a carved-stone feel (deterministic LCG)
    let mut seed: u32 = 0x9E37_79B9 ^ spec.slots;
    let mut rnd = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f64 / (1u32 << 24) as f64
    };
    ctx.set_fill_style_str("rgba(90,70,40,0.07)");
    for _ in 0..(size as usize * 2) {
        let a = rnd() * TAU;
        let r = ri + rnd() * (ro - ri);
        ctx.fill_rect(c + a.cos() * r, c + a.sin() * r, 2.0 + rnd() * 3.0, 2.0 + rnd() * 3.0);
    }

    // engraved rims
    ctx.set_stroke_style_str("#8f8160");
    ctx.set_line_width((k * 0.06).max(3.0));
    for r in [ri + k * 0.12, ro - k * 0.12] {
        ctx.begin_path();
        ctx.arc(c, c, r, 0.0, TAU)?;
        ctx.stroke();
    }

    let step = TAU / spec.slots as f64;
    let radial_px = (ro - ri) * 0.76;
    let tang_px = (step * rm).min(GATE_WINDOW * k) * 0.8;

    for i in 0..spec.slots {
        let a = i as f64 * step;

        // dividers between slots
        ctx.save();
        ctx.translate(c, c)?;
        ctx.rotate(-(a + step / 2.0))?;
        ctx.set_stroke_style_str("#a89c78");
        ctx.set_line_width((k * 0.05).max(3.0));
        ctx.begin_path();
        ctx.move_to(ri + k * 0.2, 0.0);
        ctx.line_to(ro - k * 0.2, 0.0);
        ctx.stroke();
        ctx.restore();

        // slot content, upright at the gate (canvas −y = tangential "up")
        ctx.save();
        ctx.translate(c, c)?;
        ctx.rotate(-a)?;
        ctx.translate(rm, 0.0)?;
        match spec.face {
            Face::Numeral { offset } => {
                let n = i + offset;
                let (w, h) = numeral_size(n);
                let s = (radial_px / w).min(tang_px / h);
                ctx.save();
                ctx.set_global_alpha(0.35);
                draw_numeral(&ctx, n, s * 1.6, s * 1.6, s, "#000")?;
                ctx.restore();
                draw_numeral(&ctx, n, 0.0, 0.0, s, spec.ink)?;
            }
            Face::Glyph(kind) => {
                let set = glyphs.set(kind);
                let name = match kind {
                    GlyphKind::Tzolkin => TZOLKIN_NAMES[i as usize % 20],
                    GlyphKind::Haab => HAAB_NAMES[i as usize % 19],
                };
                match set.images.get(i as usize).and_then(|o| o.as_ref()) {
                    Some(img) => draw_glyph_image(&ctx, img, radial_px, tang_px)?,
                    None => draw_glyph_name(&ctx, name, radial_px, tang_px, spec.ink, !set.settled)?,
                }
            }
        }
        ctx.restore();
    }
    Ok(canvas)
}

fn draw_glyph_image(ctx: &CanvasRenderingContext2d, img: &HtmlImageElement, box_w: f64, box_h: f64) -> Result<(), JsValue> {
    let (iw, ih) = (img.natural_width().max(1) as f64, img.natural_height().max(1) as f64);
    let s = (box_w / iw).min(box_h / ih);
    let (w, h) = (iw * s, ih * s);
    // Glyph files are dark ink on white: multiply keeps the ink and drops the white paper.
    ctx.set_global_composite_operation("multiply")?;
    ctx.draw_image_with_html_image_element_and_dw_and_dh(img, -w / 2.0, -h / 2.0, w, h)?;
    ctx.set_global_composite_operation("source-over")?;
    Ok(())
}

fn draw_glyph_name(ctx: &CanvasRenderingContext2d, name: &str, box_w: f64, box_h: f64, ink: &str, loading: bool) -> Result<(), JsValue> {
    let chars = name.chars().count().max(3) as f64;
    let px = (box_h * 0.32).min(box_w / (chars * 0.62));
    ctx.set_font(&format!("bold {px:.0}px Georgia, 'Times New Roman', serif"));
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    ctx.set_global_alpha(if loading { 0.45 } else { 1.0 });
    ctx.set_fill_style_str(ink);
    ctx.fill_text(name, 0.0, 0.0)?;
    ctx.set_global_alpha(1.0);
    Ok(())
}

// ---------- labels ----------

pub fn label(doc: &Document, text: &str) -> Result<HtmlCanvasElement, JsValue> {
    let (canvas, ctx) = make_canvas(doc, 512, 128)?;
    ctx.set_fill_style_str("#14110d");
    ctx.fill_rect(0.0, 0.0, 512.0, 128.0);
    ctx.set_stroke_style_str("#a37c35");
    ctx.set_line_width(6.0);
    ctx.stroke_rect(6.0, 6.0, 500.0, 116.0);
    ctx.set_fill_style_str("#e0b441");
    ctx.set_font("bold 58px Georgia, serif");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    ctx.fill_text(text, 256.0, 68.0)?;
    Ok(canvas)
}

// ---------- UI clock (top-right) ----------

pub fn ui_clock(ctx: &CanvasRenderingContext2d, w: f64, h: f64, hms: [u32; 3]) -> Result<(), JsValue> {
    ctx.clear_rect(0.0, 0.0, w, h);
    let band = h / 3.0;
    let gold = "#eab308";
    for (k, v) in hms.iter().enumerate() {
        let (nw, nh) = numeral_size(*v);
        let s = (w * 0.62 / nw).min(band * 0.7 / nh).min(w / 110.0 * 1.3);
        draw_numeral(ctx, *v, w / 2.0, band * (k as f64 + 0.5), s, gold)?;
    }
    ctx.set_stroke_style_str("rgba(234,179,8,0.3)");
    ctx.set_line_width(1.0);
    for k in 1..3 {
        let y = (band * k as f64).round() + 0.5;
        ctx.begin_path();
        ctx.move_to(w * 0.17, y);
        ctx.line_to(w * 0.83, y);
        ctx.stroke();
    }
    Ok(())
}
