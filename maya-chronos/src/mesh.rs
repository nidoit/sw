//! CPU-side geometry generation.
//! Vertex layout (11 floats): position(3) normal(3) uv(2) color(3, sRGB 0..1).

use crate::math::Vec3;
use std::f32::consts::TAU;

pub const FLOATS_PER_VERTEX: usize = 11;

#[derive(Default)]
pub struct MeshBuilder {
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
}

pub type Rgb = [f32; 3];

pub const fn hex(c: u32) -> Rgb {
    [
        ((c >> 16) & 0xff) as f32 / 255.0,
        ((c >> 8) & 0xff) as f32 / 255.0,
        (c & 0xff) as f32 / 255.0,
    ]
}

/// Point on a circle in the XZ plane. Angle increases counter-clockwise seen from +Y,
/// which is the same direction as a positive three.js-style `rotation.y`.
fn circle(r: f32, a: f32, y: f32) -> Vec3 {
    Vec3::new(a.cos() * r, y, -a.sin() * r)
}

impl MeshBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vertex(&mut self, p: Vec3, n: Vec3, uv: [f32; 2], c: Rgb) -> u32 {
        let idx = (self.vertices.len() / FLOATS_PER_VERTEX) as u32;
        self.vertices
            .extend_from_slice(&[p.x, p.y, p.z, n.x, n.y, n.z, uv[0], uv[1], c[0], c[1], c[2]]);
        idx
    }

    pub fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend_from_slice(&[a, b, c]);
    }

    pub fn quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.tri(a, b, c);
        self.tri(a, c, d);
    }

    /// Flat annulus facing +Y. UVs map the disc of radius `tex_r` onto the unit square:
    /// u = 0.5 + x / 2R, v = 0.5 + z / 2R  (v = 0 is the top row of the source canvas).
    pub fn annulus_top(&mut self, ri: f32, ro: f32, y: f32, seg: u32, tex_r: f32, c: Rgb) {
        let n = Vec3::new(0.0, 1.0, 0.0);
        let uv = |p: Vec3| [0.5 + p.x / (2.0 * tex_r), 0.5 + p.z / (2.0 * tex_r)];
        for i in 0..seg {
            let a0 = i as f32 / seg as f32 * TAU;
            let a1 = (i + 1) as f32 / seg as f32 * TAU;
            let (p0, p1, p2, p3) = (circle(ri, a0, y), circle(ro, a0, y), circle(ro, a1, y), circle(ri, a1, y));
            let i0 = self.vertex(p0, n, uv(p0), c);
            let i1 = self.vertex(p1, n, uv(p1), c);
            let i2 = self.vertex(p2, n, uv(p2), c);
            let i3 = self.vertex(p3, n, uv(p3), c);
            self.quad(i0, i1, i2, i3);
        }
    }

    pub fn annulus_bottom(&mut self, ri: f32, ro: f32, y: f32, seg: u32, c: Rgb) {
        let n = Vec3::new(0.0, -1.0, 0.0);
        for i in 0..seg {
            let a0 = i as f32 / seg as f32 * TAU;
            let a1 = (i + 1) as f32 / seg as f32 * TAU;
            let i0 = self.vertex(circle(ri, a0, y), n, [0.0; 2], c);
            let i1 = self.vertex(circle(ri, a1, y), n, [0.0; 2], c);
            let i2 = self.vertex(circle(ro, a1, y), n, [0.0; 2], c);
            let i3 = self.vertex(circle(ro, a0, y), n, [0.0; 2], c);
            self.quad(i0, i1, i2, i3);
        }
    }

    /// Vertical cylindrical wall between y0 (bottom) and y1 (top).
    pub fn tube_wall(&mut self, r: f32, y0: f32, y1: f32, seg: u32, outward: bool, c: Rgb) {
        let sign = if outward { 1.0 } else { -1.0 };
        for i in 0..seg {
            let a0 = i as f32 / seg as f32 * TAU;
            let a1 = (i + 1) as f32 / seg as f32 * TAU;
            let n0 = Vec3::new(a0.cos() * sign, 0.0, -a0.sin() * sign);
            let n1 = Vec3::new(a1.cos() * sign, 0.0, -a1.sin() * sign);
            let i0 = self.vertex(circle(r, a0, y0), n0, [0.0; 2], c);
            let i1 = self.vertex(circle(r, a1, y0), n1, [0.0; 2], c);
            let i2 = self.vertex(circle(r, a1, y1), n1, [0.0; 2], c);
            let i3 = self.vertex(circle(r, a0, y1), n0, [0.0; 2], c);
            self.quad(i0, i1, i2, i3);
        }
    }

    pub fn disc(&mut self, r: f32, y: f32, seg: u32, up: bool, c: Rgb) {
        let n = Vec3::new(0.0, if up { 1.0 } else { -1.0 }, 0.0);
        let center = self.vertex(Vec3::new(0.0, y, 0.0), n, [0.5, 0.5], c);
        for i in 0..seg {
            let a0 = i as f32 / seg as f32 * TAU;
            let a1 = (i + 1) as f32 / seg as f32 * TAU;
            let i0 = self.vertex(circle(r, a0, y), n, [0.0; 2], c);
            let i1 = self.vertex(circle(r, a1, y), n, [0.0; 2], c);
            self.tri(center, i0, i1);
        }
    }

    pub fn cylinder(&mut self, r: f32, y0: f32, y1: f32, seg: u32, c: Rgb) {
        self.tube_wall(r, y0, y1, seg, true, c);
        self.disc(r, y1, seg, true, c);
        self.disc(r, y0, seg, false, c);
    }

    /// Axis-aligned box of `size`, rotated about Y by `rot_y`, centred at `center`.
    pub fn cuboid(&mut self, center: Vec3, size: Vec3, rot_y: f32, c: Rgb) {
        let h = Vec3::new(size.x / 2.0, size.y / 2.0, size.z / 2.0);
        // (normal, u axis, v axis)
        let faces = [
            (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, 1.0, 0.0)),
            (Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 1.0, 0.0)),
            (Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -1.0)),
            (Vec3::new(0.0, -1.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
            (Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
            (Vec3::new(0.0, 0.0, -1.0), Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
        ];
        let scale = |v: Vec3| Vec3::new(v.x * h.x, v.y * h.y, v.z * h.z);
        for (n, u, v) in faces {
            let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
            let mut ids = [0u32; 4];
            for (k, (cu, cv)) in corners.iter().enumerate() {
                let local = Vec3::new(
                    n.x + u.x * cu + v.x * cv,
                    n.y + u.y * cu + v.y * cv,
                    n.z + u.z * cu + v.z * cv,
                );
                let p = scale(local).rotate_y(rot_y);
                let p = Vec3::new(p.x + center.x, p.y + center.y, p.z + center.z);
                ids[k] = self.vertex(p, n.rotate_y(rot_y), [0.0; 2], c);
            }
            self.quad(ids[0], ids[1], ids[2], ids[3]);
        }
    }

    /// Four-sided pyramid (radius r, height h) whose tip points along +X (`dir` = 1) or -X (`dir` = -1).
    pub fn pyramid_x(&mut self, base_center: Vec3, r: f32, h: f32, dir: f32, c: Rgb) {
        // Build pointing +Y around origin, then rotate about Z so +Y → ±X.
        let rot = if dir > 0.0 { -std::f32::consts::FRAC_PI_2 } else { std::f32::consts::FRAC_PI_2 };
        let place = |p: Vec3| {
            let q = p.rotate_z(rot);
            Vec3::new(q.x + base_center.x, q.y + base_center.y, q.z + base_center.z)
        };
        let tip = place(Vec3::new(0.0, h, 0.0));
        let base: Vec<Vec3> = (0..4)
            .map(|i| {
                let a = i as f32 / 4.0 * TAU + std::f32::consts::FRAC_PI_4;
                place(circle(r, a, 0.0))
            })
            .collect();
        let centroid = base_center.lerp(tip, 0.25);
        for i in 0..4 {
            let a = base[i];
            let b = base[(i + 1) % 4];
            let mut n = b.sub(a).cross(tip.sub(a)).normalize();
            let face_mid = Vec3::new((a.x + b.x + tip.x) / 3.0, (a.y + b.y + tip.y) / 3.0, (a.z + b.z + tip.z) / 3.0);
            if n.dot(face_mid.sub(centroid)) < 0.0 {
                n = Vec3::new(-n.x, -n.y, -n.z);
            }
            let ia = self.vertex(a, n, [0.0; 2], c);
            let ib = self.vertex(b, n, [0.0; 2], c);
            let it = self.vertex(tip, n, [0.0; 2], c);
            self.tri(ia, ib, it);
        }
        let bn = base_center.sub(tip).normalize();
        let ids: Vec<u32> = base.iter().map(|p| self.vertex(*p, bn, [0.0; 2], c)).collect();
        self.quad(ids[0], ids[1], ids[2], ids[3]);
    }

    /// Horizontal textured plane facing +Y, width along X, depth along Z.
    /// u: 0→1 along +X, v: 0→1 along +Z (canvas top row = far side, reads upright from +Z).
    pub fn plane_xz(&mut self, center: Vec3, w: f32, d: f32, c: Rgb) {
        let n = Vec3::new(0.0, 1.0, 0.0);
        let p = |dx: f32, dz: f32| Vec3::new(center.x + dx, center.y, center.z + dz);
        let a = self.vertex(p(-w / 2.0, -d / 2.0), n, [0.0, 0.0], c);
        let b = self.vertex(p(w / 2.0, -d / 2.0), n, [1.0, 0.0], c);
        let cc = self.vertex(p(w / 2.0, d / 2.0), n, [1.0, 1.0], c);
        let dd = self.vertex(p(-w / 2.0, d / 2.0), n, [0.0, 1.0], c);
        self.quad(a, b, cc, dd);
    }
}
