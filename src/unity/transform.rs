//! UnityPlayer's TransformHierarchy SSE math, lane for lane: InverseTransformPositionAndRotation (1800d7d70, the
//! world→local step of Transform::SetPositionAndRotation), SetLocalTR's rotation normalize (1800ca4c0) and the
//! local→world reads CalculateGlobalPosition (1800d5de0, = Transform::GetPosition) and CalculateGlobalRotation
//! (1800d6240). rcpps is the CPU's own approximation, as in the game.
#![allow(unused_unsafe)]
use core::arch::x86_64::*;

/// A hierarchy node's local translation, rotation and scale (t.w and s.w are 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trs {
    pub t: [f32; 4],
    pub q: [f32; 4],
    pub s: [f32; 4],
}

/// pshufd.
macro_rules! sh {
    ($v:expr, $i:literal) => {
        _mm_castsi128_ps(_mm_shuffle_epi32(_mm_castps_si128($v), $i))
    };
}

fn ld(v: [f32; 4]) -> __m128 {
    unsafe { _mm_loadu_ps(v.as_ptr()) }
}
fn k(b: [u32; 4]) -> __m128 {
    ld(b.map(f32::from_bits))
}
fn out(v: __m128) -> [f32; 4] {
    let mut o = [0.0; 4];
    unsafe { _mm_storeu_ps(o.as_mut_ptr(), v) };
    o
}

const K20: [u32; 4] = [0xc0000000, 0x40000000, 0xc0000000, 0];
const KF0: [u32; 4] = [0xc0000000, 0xc0000000, 0x40000000, 0];
const K30: [u32; 4] = [0x40000000, 0xc0000000, 0xc0000000, 0];
const CONJ: [u32; 4] = [0x80000000, 0x80000000, 0x80000000, 0];
const SIGN: [u32; 4] = [0x80000000; 4];

/// The sign flips a negative scale puts on a rotation's x/y/z (products of the other two axes' signs).
fn scale_flip(s: __m128) -> __m128 {
    unsafe {
        let x1 = _mm_xor_ps(k([0x3f800000; 4]), _mm_and_ps(s, k(SIGN)));
        let x4 = _mm_mul_ps(sh!(x1, 0x41), sh!(x1, 0x9a));
        _mm_and_ps(_mm_andnot_ps(k([0, 0, 0, 0xffffffff]), x4), k(SIGN))
    }
}

/// InverseTransformPositionAndRotation through one node `n` (the caller recurses for deeper parents first).
pub fn inverse(n: &Trs, pos: [f32; 4], rot: [f32; 4]) -> ([f32; 4], [f32; 4]) {
    unsafe {
        let s = ld(n.s);
        let x11 = _mm_sub_ps(ld(pos), ld(n.t));
        let x7 = _mm_xor_ps(ld(n.q), k(CONJ));
        let tiny = _mm_cmplt_ps(_mm_and_ps(s, k([0x7fffffff; 4])), k([0x3089705f; 4]));
        let r0 = _mm_rcp_ps(s);
        let x5 = sh!(x7, 0x8e);
        let x8 = _mm_mul_ps(_mm_mul_ps(sh!(x7, 0xaa), k(K30)), x5);
        let r1 = _mm_mul_ps(_mm_sub_ps(k([0x40000002; 4]), _mm_mul_ps(r0, s)), r0);
        let r2 = _mm_mul_ps(r1, _mm_sub_ps(k([0x40000000; 4]), _mm_mul_ps(r1, s)));
        let ok = _mm_cmpeq_ps(r2, r2);
        let inv = _mm_andnot_ps(tiny, _mm_or_ps(_mm_and_ps(ok, r2), _mm_andnot_ps(ok, r0)));
        let x2 = sh!(x7, 0xdb);
        let x0 = sh!(x7, 0x71);
        let x6 = _mm_sub_ps(_mm_mul_ps(_mm_mul_ps(sh!(x7, 0xaa), k(KF0)), x2), _mm_mul_ps(_mm_mul_ps(sh!(x7, 0x00), k(K20)), x0));
        let x10 = _mm_sub_ps(_mm_mul_ps(_mm_mul_ps(sh!(x7, 0x55), k(K20)), x0), x8);
        let x9 = _mm_sub_ps(_mm_mul_ps(_mm_mul_ps(k(K30), sh!(x7, 0x00)), x5), _mm_mul_ps(_mm_mul_ps(sh!(x7, 0x55), k(KF0)), x2));
        let x6 = _mm_add_ps(_mm_mul_ps(x6, sh!(x11, 0x55)), _mm_mul_ps(x9, sh!(x11, 0xaa)));
        let x10 = _mm_add_ps(_mm_add_ps(_mm_mul_ps(x10, sh!(x11, 0x00)), x11), x6);
        let pos = out(_mm_mul_ps(x10, inv));

        let r = ld(rot);
        let c = _mm_xor_ps(ld(n.q), k(CONJ));
        let x5 = _mm_mul_ps(_mm_movelh_ps(c, c), sh!(r, 0xf5));
        let x2 = _mm_mul_ps(sh!(c, 0xaf), r);
        let x0 = _mm_mul_ps(sh!(r, 0x88), c);
        let x3 = _mm_sub_ps(_mm_mul_ps(sh!(r, 0xd2), c), sh!(x0, 0x1e));
        let x3 = _mm_sub_ps(_mm_sub_ps(x3, sh!(x2, 0x8d)), sh!(x5, 0x63));
        let q = _mm_xor_ps(_mm_xor_ps(k(CONJ), sh!(x3, 0xd2)), scale_flip(s));
        (pos, out(q))
    }
}

/// SetLocalTR's rotation: each lane over the square root of its own horizontal sum; identity when tiny.
pub fn normalize(q: [f32; 4]) -> [f32; 4] {
    unsafe {
        let x4 = ld(q);
        let x0 = _mm_mul_ps(x4, x4);
        let x1 = _mm_add_ps(sh!(x0, 0x39), x0);
        let x0 = _mm_add_ps(sh!(x1, 0x4e), x1);
        let m = _mm_cmplt_ps(k([0x0da24260; 4]), x0);
        let n = _mm_div_ps(x4, _mm_sqrt_ps(x0));
        out(_mm_or_ps(_mm_and_ps(n, m), _mm_andnot_ps(m, k([0, 0, 0, 0x3f800000]))))
    }
}

/// One parent step of CalculateGlobalPosition.
pub fn global_position(p: &Trs, t: [f32; 4]) -> [f32; 4] {
    unsafe {
        let q = ld(p.q);
        let x9 = _mm_mul_ps(ld(p.s), ld(t));
        let (x2s, x0, x5) = (sh!(q, 0xdb), sh!(q, 0x71), sh!(q, 0x8e));
        let x6 = _mm_mul_ps(_mm_mul_ps(sh!(q, 0xaa), k(KF0)), x2s);
        let x1 = _mm_mul_ps(_mm_mul_ps(sh!(q, 0x55), k(KF0)), x2s);
        let x2 = _mm_sub_ps(_mm_mul_ps(_mm_mul_ps(sh!(q, 0x00), k(K30)), x5), x1);
        let x6 = _mm_sub_ps(x6, _mm_mul_ps(_mm_mul_ps(sh!(q, 0x00), k(K20)), x0));
        let x8 = _mm_sub_ps(_mm_mul_ps(_mm_mul_ps(sh!(q, 0x55), k(K20)), x0), _mm_mul_ps(_mm_mul_ps(sh!(q, 0xaa), k(K30)), x5));
        let x6 = _mm_add_ps(_mm_mul_ps(x6, sh!(x9, 0x55)), _mm_mul_ps(x2, sh!(x9, 0xaa)));
        let x8 = _mm_add_ps(_mm_add_ps(_mm_mul_ps(x8, sh!(x9, 0x00)), x9), x6);
        out(_mm_add_ps(x8, ld(p.t)))
    }
}

/// One parent step of CalculateGlobalRotation.
pub fn global_rotation(p: &Trs, q: [f32; 4]) -> [f32; 4] {
    unsafe {
        let x3 = _mm_xor_ps(scale_flip(ld(p.s)), ld(q));
        let x5 = ld(p.q);
        let x4 = _mm_mul_ps(_mm_movelh_ps(x5, x5), sh!(x3, 0xf5));
        let x2 = _mm_mul_ps(x3, sh!(x5, 0xaf));
        let x1 = _mm_mul_ps(x5, sh!(x3, 0x88));
        let x5 = _mm_sub_ps(_mm_mul_ps(x5, sh!(x3, 0xd2)), sh!(x1, 0x1e));
        let x5 = _mm_sub_ps(_mm_sub_ps(x5, sh!(x2, 0x8d)), sh!(x4, 0x63));
        out(_mm_xor_ps(sh!(x5, 0xd2), k(CONJ)))
    }
}

/// A Transform without a rigidbody of its own (a limbs' group object or the person root).
#[derive(Clone, Debug)]
pub struct Node {
    pub id: i32,
    pub trs: Trs,
    /// Index of the parent node.
    pub parent: Option<usize>,
}

/// Transform::SetPositionAndRotation on an object whose parents are `chain` (direct parent first, root last),
/// read back through Transform.position/rotation.
pub fn set_get_world(chain: &[Trs], pos: [f32; 3], rot: [f32; 4]) -> ([f32; 3], [f32; 4]) {
    // The world→local recursion reaches the root first.
    let (mut p, mut q) = ([pos[0], pos[1], pos[2], 0.0], rot);
    for n in chain.iter().rev() {
        (p, q) = inverse(n, p, q);
    }
    q = normalize(q);
    for n in chain {
        (p, q) = (global_position(n, p), global_rotation(n, q));
    }
    ([p[0], p[1], p[2]], q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_parent_round_trip() {
        let n = Trs { t: [0.0; 4], q: [0.0, 0.0, 0.0, 1.0], s: [1.0, 1.0, 1.0, 0.0] };
        let (p, q) = set_get_world(&[n, n], [0.5, -2.0, 0.0], [0.0, 0.0, 0.6, 0.8]);
        assert_eq!(p, [0.5, -2.0, 0.0]);
        assert_eq!(q[2..], [0.6, 0.8]);
    }
}
