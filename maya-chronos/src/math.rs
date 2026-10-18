//! Minimal linear algebra: column-major 4x4 matrices (WebGL convention).

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    pub fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn normalize(self) -> Vec3 {
        let l = self.dot(self).sqrt().max(1e-12);
        Vec3::new(self.x / l, self.y / l, self.z / l)
    }
    pub fn lerp(self, o: Vec3, t: f32) -> Vec3 {
        Vec3::new(
            self.x + (o.x - self.x) * t,
            self.y + (o.y - self.y) * t,
            self.z + (o.z - self.z) * t,
        )
    }
    /// Rotation about +Y (same convention as three.js `rotation.y`).
    pub fn rotate_y(self, a: f32) -> Vec3 {
        let (s, c) = a.sin_cos();
        Vec3::new(self.x * c + self.z * s, self.y, -self.x * s + self.z * c)
    }
    pub fn rotate_z(self, a: f32) -> Vec3 {
        let (s, c) = a.sin_cos();
        Vec3::new(self.x * c - self.y * s, self.x * s + self.y * c, self.z)
    }
}

pub type Mat4 = [f32; 16];

pub fn identity() -> Mat4 {
    let mut m = [0.0; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut r = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k * 4 + row] * b[col * 4 + k];
            }
            r[col * 4 + row] = s;
        }
    }
    r
}

pub fn perspective(fovy_rad: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy_rad / 2.0).tan();
    let nf = 1.0 / (near - far);
    let mut m = [0.0; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = (far + near) * nf;
    m[11] = -1.0;
    m[14] = 2.0 * far * near * nf;
    m
}

pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    let f = target.sub(eye).normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);
    let mut m = identity();
    m[0] = s.x;
    m[4] = s.y;
    m[8] = s.z;
    m[1] = u.x;
    m[5] = u.y;
    m[9] = u.z;
    m[2] = -f.x;
    m[6] = -f.y;
    m[10] = -f.z;
    m[12] = -s.dot(eye);
    m[13] = -u.dot(eye);
    m[14] = f.dot(eye);
    m
}

/// Rotation about +Y by `a` radians (three.js convention).
pub fn rotation_y(a: f32) -> Mat4 {
    let (s, c) = a.sin_cos();
    let mut m = identity();
    m[0] = c;
    m[2] = -s;
    m[8] = s;
    m[10] = c;
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_matrix_matches_vec_rotation() {
        let m = rotation_y(0.7);
        let p = Vec3::new(1.0, 2.0, 3.0);
        let q = p.rotate_y(0.7);
        let x = m[0] * p.x + m[4] * p.y + m[8] * p.z;
        let z = m[2] * p.x + m[6] * p.y + m[10] * p.z;
        assert!((x - q.x).abs() < 1e-5 && (z - q.z).abs() < 1e-5);
    }
}
