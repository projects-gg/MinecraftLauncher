//! The little linear algebra the renderer needs: 4 x 4 matrices acting on column vectors.

pub type Vec3 = [f32; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [[f32; 4]; 4]);

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);

    pub fn translation(t: Vec3) -> Mat4 {
        let mut m = Self::IDENTITY;
        m.0[0][3] = t[0];
        m.0[1][3] = t[1];
        m.0[2][3] = t[2];
        m
    }

    pub fn scale(s: Vec3) -> Mat4 {
        let mut m = Self::IDENTITY;
        m.0[0][0] = s[0];
        m.0[1][1] = s[1];
        m.0[2][2] = s[2];
        m
    }

    /// A rotation about one axis (0 x, 1 y, 2 z), in degrees.
    pub fn rotation(axis: usize, degrees: f32) -> Mat4 {
        let (s, c) = degrees.to_radians().sin_cos();
        let mut m = Self::IDENTITY;
        match axis {
            0 => {
                m.0[1][1] = c;
                m.0[1][2] = -s;
                m.0[2][1] = s;
                m.0[2][2] = c;
            }
            1 => {
                m.0[0][0] = c;
                m.0[0][2] = s;
                m.0[2][0] = -s;
                m.0[2][2] = c;
            }
            _ => {
                m.0[0][0] = c;
                m.0[0][1] = -s;
                m.0[1][0] = s;
                m.0[1][1] = c;
            }
        }
        m
    }

    /// The game's display rotation (`rotationXYZ`): Z applied first, then Y, then X.
    pub fn euler_xyz(degrees: Vec3) -> Mat4 {
        Self::rotation(0, degrees[0])
            .mul(&Self::rotation(1, degrees[1]))
            .mul(&Self::rotation(2, degrees[2]))
    }

    /// A unit quaternion `[x, y, z, w]` as a rotation matrix.
    pub fn quaternion(q: [f32; 4]) -> Mat4 {
        let [x, y, z, w] = q;
        let len = (x * x + y * y + z * z + w * w).sqrt();
        if len == 0.0 {
            return Self::IDENTITY;
        }
        let (x, y, z, w) = (x / len, y / len, z / len, w / len);
        Mat4([
            [
                1.0 - 2.0 * (y * y + z * z),
                2.0 * (x * y - z * w),
                2.0 * (x * z + y * w),
                0.0,
            ],
            [
                2.0 * (x * y + z * w),
                1.0 - 2.0 * (x * x + z * z),
                2.0 * (y * z - x * w),
                0.0,
            ],
            [
                2.0 * (x * z - y * w),
                2.0 * (y * z + x * w),
                1.0 - 2.0 * (x * x + y * y),
                0.0,
            ],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    pub fn mul(&self, other: &Mat4) -> Mat4 {
        let mut out = [[0.0f32; 4]; 4];
        for (r, row) in out.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = (0..4).map(|k| self.0[r][k] * other.0[k][c]).sum();
            }
        }
        Mat4(out)
    }

    pub fn apply(&self, p: Vec3) -> Vec3 {
        let m = &self.0;
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + m[0][3],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + m[1][3],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + m[2][3],
        ]
    }
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
