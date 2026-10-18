//! Allocation-free byte scanning helpers for JSON/TLE text handed over from JS.

pub fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= hay.len() || needle.len() > hay.len() {
        return None;
    }
    let last = hay.len() - needle.len();
    let first = needle[0];
    let mut i = from;
    while i <= last {
        if hay[i] == first && &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Parses a JSON-ish number starting at `pos` (leading whitespace, `:` and `"` are skipped).
/// Returns `(value, end)`; `null`, `"-"` and other non-numbers yield `None` with the cursor advanced.
pub fn number(b: &[u8], mut pos: usize) -> (Option<f64>, usize) {
    while pos < b.len() && matches!(b[pos], b' ' | b'\t' | b'\r' | b'\n' | b':' | b'"' | b',' | b'[') {
        pos += 1;
    }
    let start = pos;
    let mut neg = false;
    if pos < b.len() && (b[pos] == b'-' || b[pos] == b'+') {
        neg = b[pos] == b'-';
        pos += 1;
    }
    let mut mant: f64 = 0.0;
    let mut digits = 0;
    while pos < b.len() && b[pos].is_ascii_digit() {
        mant = mant * 10.0 + (b[pos] - b'0') as f64;
        pos += 1;
        digits += 1;
    }
    let mut scale = 0i32;
    if pos < b.len() && b[pos] == b'.' {
        pos += 1;
        while pos < b.len() && b[pos].is_ascii_digit() {
            mant = mant * 10.0 + (b[pos] - b'0') as f64;
            scale -= 1;
            pos += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return (None, if pos == start { start + 1 } else { pos });
    }
    if pos < b.len() && (b[pos] == b'e' || b[pos] == b'E') {
        pos += 1;
        let mut eneg = false;
        if pos < b.len() && (b[pos] == b'-' || b[pos] == b'+') {
            eneg = b[pos] == b'-';
            pos += 1;
        }
        let mut e = 0i32;
        while pos < b.len() && b[pos].is_ascii_digit() {
            e = e * 10 + (b[pos] - b'0') as i32;
            pos += 1;
        }
        scale += if eneg { -e } else { e };
    }
    let v = mant * 10f64.powi(scale);
    (Some(if neg { -v } else { v }), pos)
}

/// Plain decimal field of a fixed-column record (TLE). Blank → 0.
pub fn field(line: &[u8], a: usize, b: usize) -> f64 {
    let b = b.min(line.len());
    if a >= b {
        return 0.0;
    }
    number(&line[a..b], 0).0.unwrap_or(0.0)
}

/// TLE "assumed decimal point" exponent field, e.g. ` 34123-4` → 0.34123e-4.
pub fn tle_exp(line: &[u8], a: usize, b: usize) -> f64 {
    let b = b.min(line.len());
    if a >= b {
        return 0.0;
    }
    let s = &line[a..b];
    let mut i = 0;
    while i < s.len() && s[i] == b' ' {
        i += 1;
    }
    let mut sign = 1.0;
    if i < s.len() && (s[i] == b'-' || s[i] == b'+') {
        if s[i] == b'-' {
            sign = -1.0;
        }
        i += 1;
    }
    let mut mant = 0.0;
    let mut div = 1.0;
    while i < s.len() && s[i].is_ascii_digit() {
        mant = mant * 10.0 + (s[i] - b'0') as f64;
        div *= 10.0;
        i += 1;
    }
    let mut esign = 1;
    if i < s.len() && (s[i] == b'-' || s[i] == b'+') {
        if s[i] == b'-' {
            esign = -1;
        }
        i += 1;
    }
    let mut e = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        e = e * 10 + (s[i] - b'0') as i32;
        i += 1;
    }
    sign * mant / div * 10f64.powi(esign * e)
}
