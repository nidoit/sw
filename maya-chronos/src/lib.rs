//! MAYA CHRONOS — 3D Maya calendar + real-time clock in Rust/WASM (WebGL2).
//!
//! Rings, inside → out:
//!   Tzolk'in number (1–13) · Tzolk'in day sign (20) · Haab' day (0–19) · Haab' month (19)
//!   · hours (24) · minutes (60) · seconds (60)
//! Read today's values through the gold gate on the +X axis.

pub mod anim;
pub mod calendar;
pub mod math;
pub mod mesh;

mod app;
pub mod draw2d;
pub mod gl;
pub mod glyphs;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_panic_hook();
    app::run()
}

fn console_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("panic: {info}").into());
    }));
}
