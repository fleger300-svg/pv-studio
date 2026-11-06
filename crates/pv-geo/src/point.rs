use std::ops::{Add, Mul, Neg, Sub};

use serde::{Deserialize, Serialize};

/// Point or vector in the roof frame, metres.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Vec2 {
    pub u: f64,
    pub v: f64,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { u: 0.0, v: 0.0 };

    #[must_use]
    pub const fn new(u: f64, v: f64) -> Self {
        Self { u, v }
    }

    #[must_use]
    pub fn dot(self, o: Vec2) -> f64 {
        self.u * o.u + self.v * o.v
    }

    /// z-component of the 3D cross product: > 0 when `o` is counter-clockwise from `self`.
    #[must_use]
    pub fn cross(self, o: Vec2) -> f64 {
        self.u * o.v - self.v * o.u
    }

    #[must_use]
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    #[must_use]
    pub fn distance(self, o: Vec2) -> f64 {
        (self - o).length()
    }

    /// Rotated counter-clockwise by `angle_rad`.
    #[must_use]
    pub fn rotated(self, angle_rad: f64) -> Vec2 {
        let (s, c) = angle_rad.sin_cos();
        Vec2::new(c * self.u - s * self.v, s * self.u + c * self.v)
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.u.is_finite() && self.v.is_finite()
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.u + o.u, self.v + o.v)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.u - o.u, self.v - o.v)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f64) -> Vec2 {
        Vec2::new(self.u * s, self.v * s)
    }
}

impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2::new(-self.u, -self.v)
    }
}
