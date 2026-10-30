//! Box2D's vector math as Unity 2020.3's UnityPlayer.dll compiles it (SSE, no fused multiply-add). Every expression
//! keeps the native operand grouping; addition order only where the decompiled code shows it.

use std::ops::{Add, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// b2Vec2::Normalize (180bae620): the length, or 0 (unchanged) below f32 epsilon.
    pub fn normalize(&mut self) -> f32 {
        let length = self.length();
        if length < f32::EPSILON {
            return 0.0;
        }
        let inv = 1.0 / length;
        self.x *= inv;
        self.y *= inv;
        length
    }

    pub fn normalized(mut self) -> Self {
        self.normalize();
        self
    }

    /// The perpendicular b2Cross(v, 1): (y, -x).
    pub fn skew_neg(self) -> Self {
        Self::new(self.y, -self.x)
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y)
    }
}

impl Neg for Vec2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s)
    }
}

pub fn dot(a: Vec2, b: Vec2) -> f32 {
    a.x * b.x + a.y * b.y
}

pub fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

/// b2Cross(s, v): (-s * v.y, s * v.x).
pub fn cross_sv(s: f32, v: Vec2) -> Vec2 {
    Vec2::new(-s * v.y, s * v.x)
}

pub fn distance_squared(a: Vec2, b: Vec2) -> f32 {
    let d = a - b;
    d.length_squared()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rot {
    pub s: f32,
    pub c: f32,
}

impl Default for Rot {
    fn default() -> Self {
        Self { s: 0.0, c: 1.0 }
    }
}

impl Rot {
    pub fn new(angle: f32) -> Self {
        Self { s: angle.sin(), c: angle.cos() }
    }

    /// b2Mul(q, v).
    pub fn apply(self, v: Vec2) -> Vec2 {
        Vec2::new(self.c * v.x - self.s * v.y, self.s * v.x + self.c * v.y)
    }

    /// b2MulT(q, v).
    pub fn apply_t(self, v: Vec2) -> Vec2 {
        Vec2::new(self.c * v.x + self.s * v.y, -self.s * v.x + self.c * v.y)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transform {
    pub p: Vec2,
    pub q: Rot,
}

impl Transform {
    pub fn new(p: Vec2, angle: f32) -> Self {
        Self { p, q: Rot::new(angle) }
    }

    /// The identity transform (position 0, rotation s 0 / c 1).
    pub fn default_identity() -> Self {
        Self { p: Vec2::new(0.0, 0.0), q: Rot { s: 0.0, c: 1.0 } }
    }

    /// b2Mul(xf, v): (c x - s y) + p.x, (s x + c y) + p.y.
    pub fn apply(&self, v: Vec2) -> Vec2 {
        Vec2::new((self.q.c * v.x - self.q.s * v.y) + self.p.x, (self.q.s * v.x + self.q.c * v.y) + self.p.y)
    }

    /// b2MulT(xf, v): the offset from p, turned back.
    pub fn apply_t(&self, v: Vec2) -> Vec2 {
        let p = v - self.p;
        Vec2::new(self.q.c * p.x + self.q.s * p.y, -self.q.s * p.x + self.q.c * p.y)
    }
}
