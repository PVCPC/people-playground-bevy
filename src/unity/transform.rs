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

/// One node of Transform::TransformVector (1807f6f50): `global_position` without the translation (no `+ t`, so a
/// -0 stays -0).
pub fn global_vector(p: &Trs, t: [f32; 4]) -> [f32; 4] {
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
        let x8 = _mm_add_ps(_mm_mul_ps(x8, sh!(x9, 0x00)), x9);
        out(_mm_add_ps(x8, x6))
    }
}

/// One node of TransformDirection (1807f6560): a parent's negative scale axes flip the direction (`s`; None for
/// the object itself), then the node's rotation turns it.
pub fn rotate_direction(q: [f32; 4], s: Option<[f32; 4]>, v: [f32; 4]) -> [f32; 4] {
    unsafe {
        let mut v = ld(v);
        if let Some(s) = s {
            let (s, z) = (ld(s), _mm_setzero_ps());
            let sign = _mm_sub_epi32(_mm_castps_si128(_mm_cmplt_ps(s, z)), _mm_castps_si128(_mm_cmplt_ps(z, s)));
            v = _mm_xor_ps(v, _mm_and_ps(_mm_cvtepi32_ps(sign), k(SIGN)));
        }
        let q = ld(q);
        let (x2s, x0, x5) = (sh!(q, 0xdb), sh!(q, 0x71), sh!(q, 0x8e));
        let x6 = _mm_sub_ps(_mm_mul_ps(_mm_mul_ps(sh!(q, 0xaa), k(KF0)), x2s), _mm_mul_ps(_mm_mul_ps(sh!(q, 0x00), k(K20)), x0));
        let x2 = _mm_sub_ps(_mm_mul_ps(_mm_mul_ps(sh!(q, 0x00), k(K30)), x5), _mm_mul_ps(_mm_mul_ps(sh!(q, 0x55), k(KF0)), x2s));
        let x8 = _mm_sub_ps(_mm_mul_ps(_mm_mul_ps(sh!(q, 0x55), k(K20)), x0), _mm_mul_ps(_mm_mul_ps(sh!(q, 0xaa), k(K30)), x5));
        let x6 = _mm_add_ps(_mm_mul_ps(x6, sh!(v, 0x55)), _mm_mul_ps(x2, sh!(v, 0xaa)));
        let x8 = _mm_add_ps(_mm_mul_ps(x8, sh!(v, 0x00)), v);
        out(_mm_add_ps(x6, x8))
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

/// A rotation's 3x3 matrix columns, as CalculateGlobalRS / CalculateGlobalSM build them.
fn rot_columns(q: __m128) -> [__m128; 3] {
    unsafe {
        let (x3, x5, x4) = (sh!(q, 0x71), sh!(q, 0x8e), sh!(q, 0xdb));
        let (xx, yy, zz) = (sh!(q, 0x00), sh!(q, 0x55), sh!(q, 0xaa));
        let c0 = _mm_add_ps(
            _mm_add_ps(_mm_mul_ps(_mm_mul_ps(k([0xc0000000, 0x40000000, 0xc0000000, 0]), yy), x3), _mm_mul_ps(_mm_mul_ps(k([0xc0000000, 0x40000000, 0x40000000, 0]), zz), x5)),
            k([0x3f800000, 0, 0, 0]),
        );
        let c1 = _mm_add_ps(
            _mm_add_ps(_mm_mul_ps(_mm_mul_ps(k([0xc0000000, 0xc0000000, 0x40000000, 0]), zz), x4), _mm_mul_ps(_mm_mul_ps(k([0x40000000, 0xc0000000, 0x40000000, 0]), xx), x3)),
            k([0, 0x3f800000, 0, 0]),
        );
        let c2 = _mm_add_ps(
            _mm_add_ps(_mm_mul_ps(_mm_mul_ps(k([0x40000000, 0xc0000000, 0xc0000000, 0]), xx), x5), _mm_mul_ps(_mm_mul_ps(k([0x40000000, 0x40000000, 0xc0000000, 0]), yy), x4)),
            k([0, 0, 0x3f800000, 0]),
        );
        [c0, c1, c2]
    }
}

/// a × m by columns: a0·m.x + (a1·m.y + a2·m.z).
fn mat_mul(a: &[__m128; 3], m: &[__m128; 3]) -> [__m128; 3] {
    unsafe { m.map(|c| _mm_add_ps(_mm_mul_ps(a[0], sh!(c, 0x00)), _mm_add_ps(_mm_mul_ps(a[1], sh!(c, 0x55)), _mm_mul_ps(a[2], sh!(c, 0xaa))))) }
}

/// Transform::GetWorldScaleLossy (1807f1960): the diagonal of CalculateGlobalSM (1800d6390), the inverse world
/// rotation times CalculateGlobalRS (1800d5f40). `nodes` is the object's own local TRS, then each parent up to
/// the root; `rot` its world rotation (CalculateGlobalRotation).
pub fn lossy_scale(nodes: &[Trs], rot: [f32; 4]) -> [f32; 3] {
    unsafe {
        let scaled = |n: &Trs| {
            let [c0, c1, c2] = rot_columns(ld(n.q));
            let s = ld(n.s);
            [_mm_mul_ps(c0, sh!(s, 0x00)), _mm_mul_ps(c1, sh!(s, 0x55)), _mm_mul_ps(c2, sh!(s, 0xaa))]
        };
        let mut m = scaled(&nodes[0]);
        for n in &nodes[1..] {
            m = mat_mul(&scaled(n), &m);
        }
        let inv = rot_columns(_mm_xor_ps(ld(rot), k(CONJ)));
        let sm = mat_mul(&inv, &m);
        [out(sm[0])[0], out(sm[1])[1], out(sm[2])[2]]
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

/// Transform::SetPositionAndRotation on an object whose parents are `chain` (direct parent first, root last): the
/// stored local position and rotation, and the world pose Transform.position/rotation read back.
pub fn set_world(chain: &[Trs], pos: [f32; 3], rot: [f32; 4]) -> (([f32; 3], [f32; 4]), ([f32; 3], [f32; 4])) {
    // The world→local recursion reaches the root first.
    let (mut p, mut q) = ([pos[0], pos[1], pos[2], 0.0], rot);
    for n in chain.iter().rev() {
        (p, q) = inverse(n, p, q);
    }
    q = normalize(q);
    let local = ([p[0], p[1], p[2]], q);
    for n in chain {
        (p, q) = (global_position(n, p), global_rotation(n, q));
    }
    (local, ([p[0], p[1], p[2]], q))
}

/// sqrtf (181592ae8): sqrtss for the finite non-negative inputs used here.
fn sqrtf(x: f32) -> f32 {
    x.sqrt()
}

/// Matrix3x3f::SetFromToRotation (18074cca0), scalar SSE, the 9 floats in memory order. `f` = from, `t` = to, both
/// unit length.
fn set_from_to_rotation(f: [f32; 3], t: [f32; 3]) -> [f32; 9] {
    let (fx, fy, fz) = (f[0], f[1], f[2]);
    let (tx, ty, tz) = (t[0], t[1], t[2]);
    let vx = tz * fy - ty * fz;
    let vy = tx * fz - fx * tz;
    let vz = fx * ty - tx * fy;
    let e = fx * tx + ty * fy + fz * tz;
    let mut m = [0.0f32; 9];
    // cvtps2pd + comisd against +-0.99999 (1816a1450 / 1816a1458).
    if (e as f64) > f64::from_bits(0x3feffffde7210c00) {
        m[0] = 1.0;
        m[4] = 1.0;
        m[8] = 1.0;
    } else if f64::from_bits(0xbfeffffde7210c00) > (e as f64) {
        // Antiparallel: reflect about a unit vector l perpendicular to f.
        let nfy = -fy;
        let s = (fz * fz + 0.0) + nfy * nfy;
        let (mut lx, mut ly, mut lz) = (0.0f32, fz, nfy);
        if f32::from_bits(0x358637bd) > s {
            (lx, ly, lz) = (-fz, 0.0, fx);
        }
        let inv = 1.0 / sqrtf(ly * ly + lx * lx + lz * lz);
        let (lx, ly, lz) = (lx * inv, ly * inv, lz * inv);
        let n3 = -(fx * fz);
        let n4 = -(fz * fz);
        let n1 = -(lx * lx);
        let n5 = -(fy * fy);
        let n2 = -(fx * fy);
        let ux = ly * fz - lz * fy;
        let uy = lz * fx - lx * fz;
        let uz = lx * fy - ly * fx;
        let n13 = -(fz * fy);
        let n14 = -(fx * fx);
        let m0 = (n14 + ux * ux) + n1;
        let m3 = (uy * ux + n2) + -(ly * lx);
        let m6 = (ux * uz + n3) + -(lz * lx);
        let m7 = (uy * uz + n13) + -(lz * ly);
        let m8 = (uz * uz + n4) + -(lz * lz);
        let m4 = (uy * uy + n5) + -(ly * ly);
        // stores: [0]=m0 [3]=m3 [6]=m6 [7]=m7, then [1]=m3 [4]=m4 [2]=m6 [5]=m7 [8]=m8
        m[0] = m0;
        m[1] = m3;
        m[2] = m6;
        m[3] = m3;
        m[4] = m4;
        m[5] = m7;
        m[6] = m6;
        m[7] = m7;
        m[8] = m8;
    } else {
        let h = (1.0 - e) / (vx * vx + vy * vy + vz * vz);
        let x1 = vx * h;
        let x3 = vz * h;
        let x4 = vy * x1;
        let x12 = vz * x1;
        let x2 = vy * x3;
        m[0] = vx * x1 + e;
        m[1] = vz + x4;
        m[2] = x12 - vy;
        m[3] = x4 - vz;
        m[4] = (vy * h) * vy + e;
        m[5] = vx + x2;
        m[6] = vy + x12;
        m[7] = x2 - vx;
        m[8] = vz * x3 + e;
    }
    m
}

/// MatrixToQuaternion (18073f5c0): Matrix3x3f (9 floats in memory order) to a normalized (x, y, z, w).
fn matrix_to_quaternion(m: &[f32; 9]) -> [f32; 4] {
    let mut q = [0.0f32; 4];
    let tr = m[0] + m[4] + m[8];
    if tr > 0.0 {
        let root = sqrtf(tr + 1.0);
        q[3] = root * 0.5;
        let s = 0.5 / root;
        q[0] = (m[5] - m[7]) * s;
        q[1] = (m[6] - m[2]) * s;
        q[2] = (m[1] - m[3]) * s;
    } else {
        let next = [1usize, 2, 0];
        let mut i = 0;
        if m[4] > m[0] {
            i = 1;
        }
        if m[8] > m[i * 4] {
            i = 2;
        }
        let (j, kk) = (next[i], next[next[i]]);
        let root = sqrtf(m[i * 4] - m[j * 4] - m[kk * 4] + 1.0);
        q[i] = root * 0.5;
        let s = 0.5 / root;
        q[3] = (m[3 * j + kk] - m[3 * kk + j]) * s;
        q[j] = (m[3 * i + j] + m[3 * j + i]) * s;
        q[kk] = (m[3 * i + kk] + m[3 * kk + i]) * s;
    }
    let len = sqrtf(q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]);
    [q[0] / len, q[1] / len, q[2] / len, q[3] / len]
}

/// Transform.up = v, i.e. Quaternion.FromToRotation(Vector3.up, v) (Quaternion_CUSTOM_FromToRotation_Injected
/// 180973710 -> FromToQuaternionSafe 18073d8e0 -> Matrix3x3f::SetFromToRotation 18074cca0 -> MatrixToQuaternion
/// 18073f5c0). Either input shorter than 1e-5 gives identity. The caller then sets the result as the world rotation.
pub fn from_to_rotation_up(v: [f32; 3]) -> [f32; 4] {
    from_to_rotation([0.0, 1.0, 0.0], v)
}

/// Transform.right = v is Quaternion.FromToRotation(Vector3.right, v); `from_to_rotation_up` is Transform.up = v.
pub fn from_to_rotation(from: [f32; 3], v: [f32; 3]) -> [f32; 4] {
    let mf = sqrtf(from[0] * from[0] + from[1] * from[1] + from[2] * from[2]);
    let mt = sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    let eps = f32::from_bits(0x3727c5ac);
    if eps > mf || eps > mt {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let t = [v[0] / mt, v[1] / mt, v[2] / mt];
    let f = [from[0] / mf, from[1] / mf, from[2] / mf];
    matrix_to_quaternion(&set_from_to_rotation(f, t))
}

/// Renderer world bounds (the SpriteRenderer dispatch job 1806e8220, which GetWorldAABB 18069dd90 reads back from
/// renderer+0xc0 after UpdateSingleRenderer 180667030): the world matrix M (columns) and translation T of the object
/// are accumulated leaf to root as in `lossy_scale`, T = (P0·Tx + (P1·Ty + P2·Tz)) + t'; then
/// center = (M0·cx + (M1·cy + M2·cz)) + T and extents = (|M0·ex| + |M1·ey|) + |M2·ez|. `center`/`extents` are the
/// local AABB (renderer+0xd8 / +0xe4, from SpriteRenderer::UpdateLocalAABB 1806e8bd0); `chain` is the object's own
/// local TRS, then each parent up to the root.
pub fn transform_aabb(center: [f32; 3], extents: [f32; 3], chain: &[Trs]) -> ([f32; 3], [f32; 3]) {
    unsafe {
        let (m, t) = global_matrix_ps(chain);
        let c = ld([center[0], center[1], center[2], center[2]]);
        let e = ld([extents[0], extents[1], extents[2], extents[2]]);
        let ctr = _mm_add_ps(_mm_add_ps(_mm_mul_ps(m[0], sh!(c, 0x00)), _mm_add_ps(_mm_mul_ps(m[1], sh!(c, 0x55)), _mm_mul_ps(m[2], sh!(c, 0xaa)))), t);
        let ab = |v: __m128| _mm_and_ps(v, k([0x7fffffff; 4]));
        let ext = _mm_add_ps(_mm_add_ps(ab(_mm_mul_ps(sh!(e, 0x00), m[0])), ab(_mm_mul_ps(sh!(e, 0x55), m[1]))), ab(_mm_mul_ps(sh!(e, 0xaa), m[2])));
        let (c, e) = (out(ctr), out(ext));
        ([c[0], c[1], c[2]], [e[0], e[1], e[2]])
    }
}

fn global_matrix_ps(chain: &[Trs]) -> ([__m128; 3], __m128) {
    unsafe {
        let scaled = |n: &Trs| {
            let [c0, c1, c2] = rot_columns(ld(n.q));
            let s = ld(n.s);
            [_mm_mul_ps(c0, sh!(s, 0x00)), _mm_mul_ps(c1, sh!(s, 0x55)), _mm_mul_ps(c2, sh!(s, 0xaa))]
        };
        let mut m = scaled(&chain[0]);
        let mut t = ld(chain[0].t);
        for n in &chain[1..] {
            let p = scaled(n);
            t = _mm_add_ps(_mm_add_ps(_mm_mul_ps(p[0], sh!(t, 0x00)), _mm_add_ps(_mm_mul_ps(p[1], sh!(t, 0x55)), _mm_mul_ps(p[2], sh!(t, 0xaa)))), ld(n.t));
            m = mat_mul(&p, &m);
        }
        (m, t)
    }
}

/// Transform::GetLocalToWorldMatrix (1807f1130 -> CalculateGlobalMatrix 1801a56e0), column-major: the rotation-scale
/// columns with w cleared, then the translation with w = 1. `chain` is the object's own local TRS, then each parent.
pub fn local_to_world(chain: &[Trs]) -> [f32; 16] {
    let (m, t) = global_matrix_ps(chain);
    let mut out16 = [0.0f32; 16];
    for (c, col) in m.iter().enumerate() {
        out16[c * 4..c * 4 + 3].copy_from_slice(&out(*col)[..3]);
    }
    out16[12..15].copy_from_slice(&out(t)[..3]);
    out16[15] = 1.0;
    out16
}

/// QuaternionToMatrix (180740e60), scalar: column-major, no translation.
pub fn quaternion_to_matrix(q: [f32; 4]) -> [f32; 16] {
    let [x, y, z, w] = q;
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (xx, yy, zz) = (x * x2, y * y2, z * z2);
    let (xy, xz, yz) = (x * y2, x * z2, y * z2);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);
    let mut m = [0.0f32; 16];
    m[0] = 1.0 - (zz + yy);
    m[1] = wz + xy;
    m[2] = xz - wy;
    m[4] = xy - wz;
    m[5] = 1.0 - (zz + xx);
    m[6] = wx + yz;
    m[8] = wy + xz;
    m[9] = yz - wx;
    m[10] = 1.0 - (yy + xx);
    m[15] = 1.0;
    m
}

/// Bounds.ClosestPoint (Bounds_CUSTOM_ClosestPoint_Injected 18094a880 -> CalculateClosestPoint 180640690): the point
/// clamped into the box, and the squared distance (accumulated x, y, z); a point inside is returned as is.
pub fn bounds_closest_point(p: [f32; 3], center: [f32; 3], ext: [f32; 3]) -> ([f32; 3], f32) {
    let mut d = [p[0] - center[0], p[1] - center[1], p[2] - center[2]];
    let mut sq = 0.0f32;
    for a in 0..3 {
        let e = ext[a];
        let ne = -e;
        if ne > d[a] {
            let t = e + d[a];
            d[a] = ne;
            sq += t * t;
        } else if d[a] > e {
            let t = d[a] - e;
            d[a] = e;
            sq += t * t;
        }
    }
    // The first axis adds onto +0.0 (addss xmm1, xmm6); identical for the clamp sums.
    if sq == 0.0 {
        return (p, 0.0);
    }
    ([center[0] + d[0], center[1] + d[1], center[2] + d[2]], sq)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_parent_round_trip() {
        let n = Trs { t: [0.0; 4], q: [0.0, 0.0, 0.0, 1.0], s: [1.0, 1.0, 1.0, 0.0] };
        let (_, (p, q)) = set_world(&[n, n], [0.5, -2.0, 0.0], [0.0, 0.0, 0.6, 0.8]);
        assert_eq!(p, [0.5, -2.0, 0.0]);
        assert_eq!(q[2..], [0.6, 0.8]);
    }

    #[test]
    fn from_to_rotation_up_cases() {
        // up -> up: e = 1 > 0.99999, identity matrix, trace 3: root = 2, w = 1, s = 0.25, x = y = z = 0, len 1.
        assert_eq!(from_to_rotation_up([0.0, 1.0, 0.0]), [0.0, 0.0, 0.0, 1.0]);
        // Tiny input: identity.
        assert_eq!(from_to_rotation_up([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0, 1.0]);
        // up -> +x: f = (0,1,0), t = (1,0,0): e = 0, v = (0,0,-1), h = 1, matrix [0,-1,0, 1,0,0, 0,0,1]; trace 1 > 0:
        // root = sqrt(2), w = root*0.5, s = 0.5/root, z = (m1 - m3)*s = -2s, then / sqrt(w^2 + z^2).
        let root = 2.0f32.sqrt();
        let (w, s) = (root * 0.5, 0.5 / root);
        let z = (-1.0f32 - 1.0) * s;
        let len = (0.0f32 * 0.0 + 0.0 * 0.0 + z * z + w * w).sqrt();
        assert_eq!(from_to_rotation_up([1.0, 0.0, 0.0]), [0.0 / len, 0.0 / len, z / len, w / len]);
        assert!((z / len + 0.70710678).abs() < 1e-6);
        // Antiparallel (branch 2) is a 180 degree turn about a perpendicular: |w| ~ 0, unit length.
        let q = from_to_rotation_up([0.0, -1.0, 0.0]);
        assert!(q[3].abs() < 1e-6 && (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] - 1.0).abs() < 1e-6, "{q:?}");
    }

    #[test]
    fn transform_aabb_hand_worked() {
        // 180 degree turn about z (q = (0,0,1,0)): c0 = (-1,0,0), c1 = (0,-1,0), c2 = (0,0,1); s = (2,1,1):
        // M0 = (-2,0,0), M1 = (0,-1,0), M2 = (0,0,1). Local center (0.5,0,0), extents (1,1,0.5).
        let leaf = Trs { t: [1.0, 2.0, 0.0, 0.0], q: [0.0, 0.0, 1.0, 0.0], s: [2.0, 1.0, 1.0, 0.0] };
        let (c, e) = transform_aabb([0.5, 0.0, 0.0], [1.0, 1.0, 0.5], &[leaf]);
        assert_eq!((c, e), ([0.0, 2.0, 0.0], [2.0, 1.0, 0.5])); // (-1,0,0) + T(1,2,0); |M0|+|M1|+0.5|M2|
        // Identity parent at t = (10,0,0): T = (11,2,0).
        let parent = Trs { t: [10.0, 0.0, 0.0, 0.0], q: [0.0, 0.0, 0.0, 1.0], s: [1.0, 1.0, 1.0, 0.0] };
        let (c, e) = transform_aabb([0.5, 0.0, 0.0], [1.0, 1.0, 0.5], &[leaf, parent]);
        assert_eq!((c, e), ([10.0, 2.0, 0.0], [2.0, 1.0, 0.5]));
    }

    #[test]
    fn closest_point_cases() {
        // Inside: unchanged, 0.
        assert_eq!(bounds_closest_point([1.0, 1.0, 0.0], [1.0, 1.0, 0.0], [2.0, 2.0, 2.0]), ([1.0, 1.0, 0.0], 0.0));
        // d = (4, -3, 0.5) vs ext (1,1,1): clamp to (1,-1,0.5), sq = 3^2 + 2^2 = 13; closest = center + d.
        assert_eq!(bounds_closest_point([5.0, -2.0, 0.5], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0]), ([2.0, 0.0, 0.5], 13.0));
    }
}
