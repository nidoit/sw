//! Loading the Tzolk'in / Haab' glyph images from Wikimedia Commons.
//!
//! Why the original page showed nothing: Commons file URLs contain a hash directory
//! `/<a>/<ab>/` where `ab` = first two hex digits of md5(file name). The hand-written
//! URLs had wrong hashes, so every request returned 404. Here we compute the hash.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::HtmlImageElement;

const COMMONS: &str = "https://upload.wikimedia.org/wikipedia/commons";

/// File-name stems on Commons, in Tzolk'in order (Imix … Ajaw).
const TZOLKIN_FILES: [&str; 20] = [
    "Imix", "Ik", "Akbal", "Kan", "Chikchan", "Kimi", "Manik", "Lamat", "Muluk", "Ok", "Chuwen",
    "Eb", "Ben", "Ix", "Men", "Kib", "Kaban", "Etznab", "Kawak", "Ajaw",
];

/// Month glyphs (vector versions), Pop … Wayeb'.
const HAAB_SVG_FILES: [&str; 19] = [
    "Maya_months_-_0_-_Pop.svg",
    "Maya_months_-_1_-_Wo.svg",
    "Maya_months_-_2_-_Sip.svg",
    "Maya-months-03-sotz'.svg",
    "Maya_months_-_4_-_Sek.svg",
    "Maya-months-05-xul.svg",
    "Maya_months_-_6_-_Yaxk'in.svg",
    "Maya_months_-_7_-_Mol.svg",
    "Maya-months-08-ch'en.svg",
    "Maya-months-09-yax.svg",
    "Maya-months-10-sak.svg",
    "Maya-months-11-kej.svg",
    "Maya-months-12-mak.svg",
    "Maya-months-13-k'ank'in.svg",
    "Maya-months-14-muwan.svg",
    "Maya-months-15-pax.svg",
    "Maya-months-16-k'ayab'.svg",
    "Maya-months-17-kumk'u.svg",
    "Maya-months-18-wayeb'.svg",
];

/// Dresden Codex versions of the month glyphs (second choice).
const HAAB_CODEX_FILES: [&str; 19] = [
    "Maya-Pop.jpg",
    "Maya-Dresden-wo.jpg",
    "Maya-Dresden-sip.jpg",
    "Sotz.jpg",
    "Maya-Dresden-Sek.jpg",
    "Xul.png",
    "Maya-Dresden-Yaxkin.jpg",
    "Maya-Mol.png",
    "Maya-Dresden-Chen.jpg",
    "Maya-Dresden-Yax.jpg",
    "Maya-Dresden-Sak.jpg",
    "Maya-Dresden-Keh.jpg",
    "Maya-Dresden-Mak.png",
    "Maya-Dresden-Kankin.png",
    "Muan.jpg",
    "Maya-Dresden-pax.jpg",
    "Maya-Dresden-Kayab.png",
    "Maya-Dresden-kumku.jpg",
    "Maya-Dresden-wayeb.jpg",
];

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// Direct URL of a Commons file. For SVGs a PNG thumbnail of `thumb_px` width is requested.
pub fn commons_url(file: &str, thumb_px: u32) -> String {
    let name = file.replace(' ', "_");
    let hash = format!("{:x}", md5::compute(name.as_bytes()));
    let (a, ab) = (&hash[..1], &hash[..2]);
    let enc = percent_encode(&name);
    if name.to_ascii_lowercase().ends_with(".svg") {
        format!("{COMMONS}/thumb/{a}/{ab}/{enc}/{thumb_px}px-{enc}.png")
    } else {
        format!("{COMMONS}/{a}/{ab}/{enc}")
    }
}

pub fn tzolkin_candidates(i: usize) -> Vec<String> {
    let stem = format!("MAYA-g-log-cal-D{:02}-{}", i + 1, TZOLKIN_FILES[i]);
    vec![
        commons_url(&format!("{stem}.png"), 0),      // inscription style
        commons_url(&format!("{stem}-cdxW.png"), 0), // codex style
    ]
}

pub fn haab_candidates(i: usize) -> Vec<String> {
    vec![
        commons_url(HAAB_SVG_FILES[i], 250),
        commons_url(HAAB_SVG_FILES[i], 330),
        commons_url(HAAB_CODEX_FILES[i], 0),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GlyphKind {
    Tzolkin,
    Haab,
}

pub struct GlyphSet {
    pub images: Vec<Option<HtmlImageElement>>,
    pending: usize,
    /// Set when every image of the set has either loaded or failed for good.
    pub settled: bool,
}

impl GlyphSet {
    fn new(n: usize) -> Self {
        GlyphSet { images: vec![None; n], pending: n, settled: false }
    }
    fn finish_one(&mut self) {
        self.pending = self.pending.saturating_sub(1);
        if self.pending == 0 {
            self.settled = true;
        }
    }
    pub fn loaded_count(&self) -> usize {
        self.images.iter().filter(|i| i.is_some()).count()
    }
}

pub struct GlyphStore {
    pub tzolkin: GlyphSet,
    pub haab: GlyphSet,
}

impl GlyphStore {
    pub fn set(&self, k: GlyphKind) -> &GlyphSet {
        match k {
            GlyphKind::Tzolkin => &self.tzolkin,
            GlyphKind::Haab => &self.haab,
        }
    }
    fn set_mut(&mut self, k: GlyphKind) -> &mut GlyphSet {
        match k {
            GlyphKind::Tzolkin => &mut self.tzolkin,
            GlyphKind::Haab => &mut self.haab,
        }
    }
}

/// Starts loading all glyphs. Each image tries its candidate URLs in order.
pub fn start_loading() -> Result<Rc<RefCell<GlyphStore>>, JsValue> {
    let store = Rc::new(RefCell::new(GlyphStore {
        tzolkin: GlyphSet::new(20),
        haab: GlyphSet::new(19),
    }));
    for i in 0..20 {
        load_one(&store, GlyphKind::Tzolkin, i, tzolkin_candidates(i))?;
    }
    for i in 0..19 {
        load_one(&store, GlyphKind::Haab, i, haab_candidates(i))?;
    }
    Ok(store)
}

fn load_one(
    store: &Rc<RefCell<GlyphStore>>,
    kind: GlyphKind,
    idx: usize,
    candidates: Vec<String>,
) -> Result<(), JsValue> {
    let img = HtmlImageElement::new()?;
    // Must be set before `src`; upload.wikimedia.org answers with
    // Access-Control-Allow-Origin: *, so the canvas will not be tainted.
    img.set_cross_origin(Some("anonymous"));
    let candidates = Rc::new(candidates);
    let attempt = Rc::new(Cell::new(0usize));

    let onload = {
        let store = store.clone();
        let img = img.clone();
        Closure::<dyn FnMut()>::new(move || {
            let mut s = store.borrow_mut();
            let set = s.set_mut(kind);
            if set.images[idx].is_none() {
                set.images[idx] = Some(img.clone());
                set.finish_one();
            }
        })
    };
    let onerror = {
        let store = store.clone();
        let img = img.clone();
        let candidates = candidates.clone();
        let attempt = attempt.clone();
        Closure::<dyn FnMut()>::new(move || {
            let next = attempt.get() + 1;
            attempt.set(next);
            if next < candidates.len() {
                img.set_src(&candidates[next]);
            } else {
                web_sys::console::warn_1(
                    &format!("glyph {kind:?}[{idx}] failed to load: {:?}", candidates).into(),
                );
                store.borrow_mut().set_mut(kind).finish_one();
            }
        })
    };
    img.set_onload(Some(onload.as_ref().unchecked_ref()));
    img.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    // These live for the whole page lifetime.
    onload.forget();
    onerror.forget();
    img.set_src(&candidates[0]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_paths_are_md5_based() {
        assert_eq!(
            commons_url("MAYA-g-log-cal-D01-Imix.png", 0),
            "https://upload.wikimedia.org/wikipedia/commons/7/7b/MAYA-g-log-cal-D01-Imix.png"
        );
        assert_eq!(
            commons_url("Maya_months_-_0_-_Pop.svg", 250),
            "https://upload.wikimedia.org/wikipedia/commons/thumb/6/65/Maya_months_-_0_-_Pop.svg/250px-Maya_months_-_0_-_Pop.svg.png"
        );
    }

    #[test]
    fn apostrophes_are_encoded() {
        let u = commons_url("Maya-months-08-ch'en.svg", 250);
        assert!(u.contains("/6/6a/Maya-months-08-ch%27en.svg/250px-Maya-months-08-ch%27en.svg.png"));
    }
}
