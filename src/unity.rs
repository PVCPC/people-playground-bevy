//! UnityEngine natives the game scripts call, rewritten from UnityPlayer.dll (Unity 2020.3).

pub mod transform;

/// The engine's 2 pi constant (0x40c90fdb).
const TWO_PI: f32 = 6.2831855;

/// UnityEngine.Random: the scripting xorshift128 state (Rand in the engine).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Random {
    pub state: [u32; 4],
}

impl Random {
    /// Random.InitState
    pub fn new(seed: i32) -> Self {
        let x = seed as u32;
        let y = x.wrapping_mul(0x6c078965).wrapping_add(1);
        let z = y.wrapping_mul(0x6c078965).wrapping_add(1);
        let w = z.wrapping_mul(0x6c078965).wrapping_add(1);
        Self { state: [x, y, z, w] }
    }

    fn next(&mut self) -> u32 {
        let [x, y, z, w] = self.state;
        let t = x << 11 ^ x;
        let next = ((w >> 11 ^ t) >> 8) ^ w ^ t;
        self.state = [y, z, w, next];
        next
    }

    /// Random.value: [0, 1] inclusive.
    pub fn value(&mut self) -> f32 {
        (self.next() & 0x7fffff) as f32 * f32::from_bits(0x34000001)
    }

    /// Random.Range(float, float)
    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        let t = self.value();
        (1.0 - t) * max + t * min
    }

    /// Random.insideUnitCircle (Random_CUSTOM_GetRandomUnitCircle)
    pub fn inside_unit_circle(&mut self) -> [f32; 2] {
        let angle = self.range(0.0, TWO_PI);
        let (cos, sin) = (cosf(angle), sinf(angle));
        let radius = self.range(0.0, 1.0).sqrt();
        [cos * radius, sin * radius]
    }

    /// RandomUnitVector(Rand&): Random.onUnitSphere
    pub fn on_unit_sphere(&mut self) -> [f32; 3] {
        let t = self.value();
        let z = (1.0 - t) - t;
        let angle = self.range(0.0, TWO_PI);
        let radius = (1.0 - z * z).sqrt();
        [cosf(angle) * radius, sinf(angle) * radius, z]
    }

    /// Random.insideUnitSphere
    pub fn inside_unit_sphere(&mut self) -> [f32; 3] {
        let v = self.on_unit_sphere();
        // ponytail: host powf; rewrite the CRT powf FMA path if a script ever calls insideUnitSphere.
        let scale = self.value().powf(f32::from_bits(0x3eaaaaab));
        [v[0] * scale, v[1] * scale, v[2] * scale]
    }

    /// Random.Range(int, int): max exclusive; min - n when min > max.
    pub fn range_int(&mut self, min: i32, max: i32) -> i32 {
        if min < max {
            min.wrapping_add((self.next() % max.wrapping_sub(min) as u32) as i32)
        } else if max < min {
            min.wrapping_sub((self.next() % min.wrapping_sub(max) as u32) as i32)
        } else {
            min
        }
    }
}

/// Ken Perlin's permutation, as PerlinNoise::p repeats it (512 entries).
const COS_C: [f64; 4] = [
    f64::from_bits(0x3fa5555555555555),
    f64::from_bits(0xbf56c16c16c16c16),
    f64::from_bits(0x3efa01a01a01a019),
    f64::from_bits(0xbe927e4fb7789f5c),
];
const SIN_C: [f64; 4] = [
    f64::from_bits(0xbfc5555555555555),
    f64::from_bits(0x3f81111111111111),
    f64::from_bits(0xbf2a01a01a01a01a),
    f64::from_bits(0x3ec71de3a556c734),
];
const PI_4: u64 = 0x3fe921fb54442d18;

/// cos kernel on [-pi/4, pi/4]; `fused` = sinf's variant, which fuses 1 - r²/2.
fn cos_kernel(r: f64, fused: bool) -> f64 {
    let r2 = r * r;
    let t = if fused { r2.mul_add(-0.5, 1.0) } else { 1.0 - r2 * 0.5 };
    let p = r2.mul_add(COS_C[3], COS_C[2]).mul_add(r2, COS_C[1]).mul_add(r2, COS_C[0]);
    p.mul_add(r2 * r2, t)
}

fn sin_kernel(r: f64) -> f64 {
    let r2 = r * r;
    let p = r2.mul_add(SIN_C[3], SIN_C[2]).mul_add(r2, SIN_C[1]).mul_add(r2, SIN_C[0]);
    p.mul_add(r * r2, r)
}

/// Cody-Waite reduction by pi/2 of |x| (FMA path): (r, quadrant).
fn reduce(a: f64) -> (f64, u32) {
    let n = a.mul_add(f64::from_bits(0x3fe45f306dc9c883), 0.5) as i32;
    let nd = n as f64;
    let r1 = (-nd).mul_add(f64::from_bits(0x3ff921fb54400000), a);
    (r1 - nd * f64::from_bits(0x3dd0b4611a626331), n as u32 & 3)
}

/// The engine's statically linked CRT cosf, FMA3 path (taken on every CPU with FMA3/AVX2).
/// ponytail: |x| >= 3373259520 needs the CRT's Payne-Hanek reduction; script callers pass angles in [0, 2pi].
pub fn cosf(x: f32) -> f32 {
    let xd = x as f64;
    let a = xd.abs();
    assert!(a < 3373259520.0, "cosf: large-argument reduction not rewritten");
    if a.to_bits() <= PI_4 {
        return if a < 0.0001220703125 {
            1.0
        } else if a < 0.0078125 {
            (-(xd * 0.5)).mul_add(xd, 1.0) as f32
        } else {
            cos_kernel(xd, false) as f32
        };
    }
    let (r, q) = reduce(a);
    let v = if q & 1 == 1 { sin_kernel(r) } else { cos_kernel(r, false) };
    (if (q + 1) & 2 != 0 { -v } else { v }) as f32
}

/// The engine's statically linked CRT sinf, FMA3 path.
/// ponytail: |x| >= 16779436 needs the CRT's Payne-Hanek reduction; script callers pass angles in [0, 2pi].
pub fn sinf(x: f32) -> f32 {
    let xd = x as f64;
    let a = xd.abs();
    assert!(a < 16779436.0, "sinf: large-argument reduction not rewritten");
    if a.to_bits() <= PI_4 {
        return if a < 0.0001220703125 {
            x
        } else if a < 0.0078125 {
            (-(xd * xd * xd)).mul_add(1.0 / 6.0, xd) as f32
        } else {
            sin_kernel(xd) as f32
        };
    }
    let (r, q) = reduce(a);
    let v = if q & 1 == 1 { cos_kernel(r, true) } else { sin_kernel(r) };
    let v = if q >= 2 { -v } else { v };
    (if xd < 0.0 { -v } else { v }) as f32
}

/// The engine's statically linked CRT acosf (181592fcc).
pub fn acosf(x: f32) -> f32 {
    let c = f32::from_bits;
    let ux = x.to_bits();
    let e = (ux >> 23) & 0xff;
    if ux & 0x7fff_ffff > 0x7f80_0000 {
        return x + x;
    }
    if e < 0x65 {
        return c(0x3fc90fdb);
    }
    if e >= 0x7f {
        return if x == 1.0 { 0.0 } else if x == -1.0 { c(0x40490fdb) } else { c(0xffc00000) };
    }
    // |x| >= 0.5 works on sqrt((1 - |x|) / 2).
    let transform = e >= 0x7e;
    let (r, s) = if transform {
        let r = (1.0 - x.abs()) * 0.5;
        (r, r.sqrt())
    } else {
        (x * x, 0.0)
    };
    let p = (((c(0xbc5b3fe1) - r * c(0x3b81ce6b)) * r - c(0x3d678bdd)) * r + c(0x3e3c94dc)) * r;
    let u = p / (c(0x3f8d6fa5) - r * c(0x3f561f0d));
    let pio2_lo = f64::from_bits(0x3c91a62633145c07);
    if !transform {
        return (f64::from_bits(0x3ff921fb54442d18) - (x as f64 - (pio2_lo - (u * x) as f64))) as f32;
    }
    if (ux as i32) < 0 {
        let v = ((u * s) as f64 - pio2_lo) + s as f64;
        return (f64::from_bits(0x400921fb54442d18) - (v + v)) as f32;
    }
    let s1 = c(s.to_bits() & 0xffff_0000);
    let t = (r - s1 * s1) / (s1 + s);
    ((s + s) * u + (t + t)) + s1 * 2.0
}

/// Checks cosf/sinf against the engine's (NativeProbe `trig`) on every angle Random.Range(0, 2pi) yields.
pub fn compare_trig(probe: &std::path::Path) -> Result<(), String> {
    let output = std::process::Command::new(probe).arg("trig").output().map_err(|e| e.to_string())?;
    let expected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let mut h: u64 = 0xcbf29ce484222325;
    let mut mix = |v: f32| {
        for byte in v.to_bits().to_le_bytes() {
            h = (h ^ byte as u64).wrapping_mul(0x100000001b3);
        }
    };
    for u in 0..1u32 << 23 {
        let t = u as f32 * f32::from_bits(0x34000001);
        let a = (1.0 - t) * TWO_PI + t * 0.0;
        mix(cosf(a));
        mix(sinf(a));
    }
    let actual = format!("{h:016x}");
    if actual != expected {
        return Err(format!("cosf/sinf differ from the engine: {actual} vs {expected}"));
    }
    println!("cosf/sinf: 2^23 angles bit-exact with the engine");
    Ok(())
}

const P: [i32; 256] = [
    151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225,
    140, 36, 103, 30, 69, 142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148,
    247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219, 203, 117, 35, 11, 32,
    57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175,
    74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122,
    60, 211, 133, 230, 220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54,
    65, 25, 63, 161, 1, 216, 80, 73, 209, 76, 132, 187, 208, 89, 18, 169,
    200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173, 186, 3, 64,
    52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212,
    207, 206, 59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170, 213,
    119, 248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43, 172, 9,
    129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185, 112, 104,
    218, 246, 97, 228, 251, 34, 242, 193, 238, 210, 144, 12, 191, 179, 162, 241,
    81, 51, 145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157,
    184, 84, 204, 176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93,
    222, 114, 67, 29, 24, 72, 243, 141, 128, 195, 78, 66, 215, 61, 156, 180,
];

fn p(i: usize) -> usize {
    P[i & 255] as usize
}

fn grad(hash: usize, x: f32, y: f32) -> f32 {
    let h = hash & 15;
    let (u, v) = if h < 8 {
        (x, if h < 4 { y } else { 0.0 })
    } else {
        (y, if h == 12 || h == 14 { x } else { 0.0 })
    };
    let u = if h & 1 != 0 { -u } else { u };
    let v = if h & 2 != 0 { -v } else { v };
    v + u
}

fn fade(t: f32) -> f32 {
    let f = 1f32.min(t);
    ((f * 6.0 - 15.0) * f + 10.0) * (f * f * f)
}

/// PerlinNoise::Noise: improved noise at z = 0 on |x|, |y|.
fn noise(x: f32, y: f32) -> f32 {
    let (x, y) = (x.abs(), y.abs());
    let (xi, yi) = (x as i32, y as i32);
    let (xf, yf) = (x - xi as f32, y - yi as f32);
    let (cx, cy) = ((xi & 255) as usize, (yi & 255) as usize);
    let a = p(cx) + cy;
    let b = p(cx + 1) + cy;
    let (aa, ab, ba, bb) = (p(a), p(a + 1), p(b), p(b + 1));
    let (u, v) = (fade(xf), fade(yf));
    let lerp = |a: f32, b: f32, t: f32| (b - a) * t + a;
    let y1 = lerp(grad(p(ab), xf, yf - 1.0), grad(p(bb), xf - 1.0, yf - 1.0), u);
    let y0 = lerp(grad(p(aa), xf, yf), grad(p(ba), xf - 1.0, yf), u);
    lerp(y0, y1, v)
}

/// Mathf.PerlinNoise
pub fn perlin_noise(x: f32, y: f32) -> f32 {
    (noise(x, y) + f32::from_bits(0x3f30a3d7)) / f32::from_bits(0x3fbdd2f2)
}

/// atan(j/256) for j = 16..=256, as doubles (UnityPlayer's CRT atan_jby256).
const ATAN_JBY256: [u64; 241] = [
    0x3faff55bb72cfde9, 0x3fb0f99ea71d52a6, 0x3fb1f86dbf082d58, 0x3fb2f719318a4a9a,
    0x3fb3f59f0e7c559d, 0x3fb4f3fd677292fb, 0x3fb5f2324fd2d7b2, 0x3fb6f03bdcea4b0c,
    0x3fb7ee182602f10e, 0x3fb8ebc54478fb28, 0x3fb9e94153cfdcf1, 0x3fbae68a71c722b8,
    0x3fbbe39ebe6f07c3, 0x3fbce07c5c3cca32, 0x3fbddd21701eba6e, 0x3fbed98c2190043a,
    0x3fbfd5ba9aac2f6d, 0x3fc068d584212b3d, 0x3fc0e6adccf40881, 0x3fc1646541060850,
    0x3fc1e1fafb043726, 0x3fc25f6e171a535c, 0x3fc2dcbdb2fba1ff, 0x3fc359e8edeb99a3,
    0x3fc3d6eee8c6626c, 0x3fc453cec6092a9e, 0x3fc4d087a9da4f17, 0x3fc54d18ba11570a,
    0x3fc5c9811e3ec269, 0x3fc645bfffb3aa73, 0x3fc6c1d4898933d8, 0x3fc73dbde8a7d201,
    0x3fc7b97b4bce5b02, 0x3fc8350be398ebc7, 0x3fc8b06ee2879c28, 0x3fc92ba37d050271,
    0x3fc9a6a8e96c8626, 0x3fca217e601081a5, 0x3fca9c231b403279, 0x3fcb1696574d780b,
    0x3fcb90d7529260a2, 0x3fcc0ae54d768466, 0x3fcc84bf8a742e6d, 0x3fccfe654e1d5395,
    0x3fcd77d5df205736, 0x3fcdf110864c9d9d, 0x3fce6a148e96ec4d, 0x3fcee2e1451d980c,
    0x3fcf5b75f92c80dd, 0x3fcfd3d1fc40dbe4, 0x3fd025fa510665b5, 0x3fd061eea03d6290,
    0x3fd09dc597d86362, 0x3fd0d97ee509acb3, 0x3fd1151a362431c9, 0x3fd150973a9ce546,
    0x3fd18bf5a30bf178, 0x3fd1c735212dd883, 0x3fd2025567e47c95, 0x3fd23d562b381041,
    0x3fd278372057ef45, 0x3fd2b2f7fd9b5fe2, 0x3fd2ed987a823cfe, 0x3fd328184fb58951,
    0x3fd362773707ebcb, 0x3fd39cb4eb76157b, 0x3fd3d6d129271134, 0x3fd410cbad6c7d32,
    0x3fd44aa436c2af09, 0x3fd4845a84d0c21b, 0x3fd4bdee586890e6, 0x3fd4f75f73869978,
    0x3fd530ad9951cd49, 0x3fd569d88e1b4cd7, 0x3fd5a2e0175e0f4e, 0x3fd5dbc3fbbe768d,
    0x3fd614840309cfe1, 0x3fd64d1ff635c1c5, 0x3fd685979f5fa6fd, 0x3fd6bdeac9cbd76c,
    0x3fd6f61941e4def0, 0x3fd72e22d53aa2a9, 0x3fd7660752817501, 0x3fd79dc6899118d1,
    0x3fd7d5604b63b3f7, 0x3fd80cd46a14b1d0, 0x3fd84422b8df95d7, 0x3fd87b4b0c1ebedb,
    0x3fd8b24d394a1b25, 0x3fd8e92916f5cde8, 0x3fd91fde7cd0c662, 0x3fd9566d43a34907,
    0x3fd98cd5454d6b18, 0x3fd9c3165cc58107, 0x3fd9f93066168001, 0x3fda2f233e5e530b,
    0x3fda64eec3cc23fc, 0x3fda9a92d59e98cf, 0x3fdad00f5422058b, 0x3fdb056420ae9343,
    0x3fdb3a911da65c6c, 0x3fdb6f962e737efb, 0x3fdba473378624a5, 0x3fdbd9281e528191,
    0x3fdc0db4c94ec9ef, 0x3fdc42191ff11eb6, 0x3fdc76550aad71f8, 0x3fdcaa6872f3631b,
    0x3fdcde53432c1350, 0x3fdd121566b7f2ad, 0x3fdd45aec9ec862b, 0x3fdd791f5a1226f4,
    0x3fddac670561bb4f, 0x3fdddf85bb026974, 0x3fde127b6b0744af, 0x3fde4548066cf51a,
    0x3fde77eb7f175a34, 0x3fdeaa65c7cf28c4, 0x3fdedcb6d43f8434, 0x3fdf0ede98f393cf,
    0x3fdf40dd0b541417, 0x3fdf72b221a4e495, 0x3fdfa45dd3029258, 0x3fdfd5e0175fdf83,
    0x3fe0039c73c1a40b, 0x3fe01c341e82422d, 0x3fe034b709250488, 0x3fe04d25314342e5,
    0x3fe0657e94db30cf, 0x3fe07dc3324e9b38, 0x3fe095f30861a58f, 0x3fe0ae0e1639866c,
    0x3fe0c6145b5b43da, 0x3fe0de05d7aa6f7c, 0x3fe0f5e28b67e295, 0x3fe10daa77307a0d,
    0x3fe1255d9bfbd2a8, 0x3fe13cfbfb1b056e, 0x3fe1548596376469, 0x3fe16bfa6f5137e1,
    0x3fe1835a88be7c13, 0x3fe19aa5e5299f99, 0x3fe1b1dc87904284, 0x3fe1c8fe7341f64f,
    0x3fe1e00babdefeb3, 0x3fe1f7043557138a, 0x3fe20de813e823b1, 0x3fe224b74c1d192a,
    0x3fe23b71e2cc9e6a, 0x3fe25217dd17e501, 0x3fe268a940696da6, 0x3fe27f261273d1b3,
    0x3fe2958e59308e30, 0x3fe2abe21aded073, 0x3fe2c2215e024465, 0x3fe2d84c2961e48b,
    0x3fe2ee628406cbca, 0x3fe30464753b090a, 0x3fe31a52048874be, 0x3fe3302b39b78856,
    0x3fe345f01cce37bb, 0x3fe35ba0b60eccce, 0x3fe3713d0df6c503, 0x3fe386c52d3db11e,
    0x3fe39c391cd41719, 0x3fe3b198e5e2564a, 0x3fe3c6e491c78dc4, 0x3fe3dc1c2a188504,
    0x3fe3f13fb89e96f4, 0x3fe4064f47569f48, 0x3fe41b4ae06fea41, 0x3fe430328e4b26d5,
    0x3fe445065b795b55, 0x3fe459c652badc7f, 0x3fe46e727efe4715, 0x3fe4830aeb5f7bfd,
    0x3fe4978fa3269ee1, 0x3fe4ac00b1c71762, 0x3fe4c05e22de94e4, 0x3fe4d4a8023414e8,
    0x3fe4e8de5bb6ec04, 0x3fe4fd013b7dd17e, 0x3fe51110adc5ed81, 0x3fe5250cbef1e9fa,
    0x3fe538f57b89061e, 0x3fe54ccaf0362c8f, 0x3fe5608d29c70c34, 0x3fe5743c352b33b9,
    0x3fe587d81f732fba, 0x3fe59b60f5cfab9d, 0x3fe5aed6c5909517, 0x3fe5c2399c244260,
    0x3fe5d58987169b18, 0x3fe5e8c6941043cf, 0x3fe5fbf0d0d5cc49, 0x3fe60f084b46e05e,
    0x3fe6220d115d7b8d, 0x3fe634ff312d1f3b, 0x3fe647deb8e20b8f, 0x3fe65aabb6c07b02,
    0x3fe66d663923e086, 0x3fe6800e4e7e2857, 0x3fe692a40556fb6a, 0x3fe6a5276c4b0575,
    0x3fe6b798920b3d98, 0x3fe6c9f7855c3198, 0x3fe6dc44551553ae, 0x3fe6ee7f10204aef,
    0x3fe700a7c5784633, 0x3fe712be84295198, 0x3fe724c35b4fae7b, 0x3fe736b65a172dff,
    0x3fe748978fba8e0f, 0x3fe75a670b82d8d8, 0x3fe76c24dcc6c6c0, 0x3fe77dd112ea22c7,
    0x3fe78f6bbd5d315e, 0x3fe7a0f4eb9c19a2, 0x3fe7b26cad2e50fd, 0x3fe7c3d311a6092b,
    0x3fe7d528289fa093, 0x3fe7e66c01c114fd, 0x3fe7f79eacb97898, 0x3fe808c03940694a,
    0x3fe819d0b7158a4c, 0x3fe82ad036000005, 0x3fe83bbec5cdee22, 0x3fe84c9c7653f7ea,
    0x3fe85d69576cc2c5, 0x3fe86e2578f87ae5, 0x3fe87ed0eadc5a2a, 0x3fe88f6bbd023118,
    0x3fe89ff5ff57f1f7, 0x3fe8b06fc1cf3dfe, 0x3fe8c0d9145cf49d, 0x3fe8d13206f8c4ca,
    0x3fe8e17aa99cc05d, 0x3fe8f1b30c44f167, 0x3fe901db3eeef187, 0x3fe911f35199833b,
    0x3fe921fb54442d18,
];

/// The CRT atan2f UnityPlayer links (QuaternionToEuler's qAtan2): double evaluation with the
/// atan_jby256 table, so it is not always the correctly rounded atan2.
pub fn atan2f(y: f32, x: f32) -> f32 {
    let (dy, dx) = (y as f64, x as f64);
    let (by, bx) = (dy.to_bits(), dx.to_bits());
    let ey = ((by >> 52) & 0x7ff) as i32;
    let ex = ((bx >> 52) & 0x7ff) as i32;
    let diff = ey - ex;
    let (ax, ay) = (dx.abs(), dy.abs());
    let (y_neg, x_neg) = (by >> 63 != 0, bx >> 63 != 0);
    let signed = |v: u32| f32::from_bits(v | if y_neg { 0x8000_0000 } else { 0 });
    if ax.is_nan() {
        return f32::from_bits(x.to_bits() | 0x40_0000);
    }
    if ay.is_nan() {
        return f32::from_bits(y.to_bits() | 0x40_0000);
    }
    if ay == 0.0 {
        return if x_neg { signed(0x4049_0fdb) } else { y };
    }
    if ax == 0.0 && y_neg {
        return f32::from_bits(0xbfc9_0fdb);
    }
    if diff > 0x1a {
        return signed(0x3fc9_0fdb);
    }
    if diff < -0xd && !x_neg {
        if diff < -0x96 {
            return signed(0);
        }
        if diff < -0x7e {
            // Result is a float denormal: round the scaled quotient by hand.
            let v = (dy * 1.2676506002282294e30) / dx;
            let vb = v.to_bits();
            let e = ((v.abs().to_bits()) >> 52) as u32;
            let mut m = 0u64;
            if e < 0x65 {
                let k = 0x65 - e;
                if k < 0x37 {
                    m = ((vb & 0x1f_ffff_ffff_ffff) | 0x10_0000_0000_0000) >> ((k - 1) & 63);
                    m = (m >> 1) + (m & 1);
                }
            } else {
                m = ((e as u64) - 100) << 52 | (vb & 0xf_ffff_ffff_ffff);
            }
            return f64::from_bits(m | (vb & 0x8000_0000_0000_0000)) as f32;
        }
        return (dy / dx) as f32;
    }
    if diff < -0x1a && x_neg {
        return signed(0x4049_0fdb);
    }
    if ay == f64::INFINITY && ax == f64::INFINITY {
        return if x_neg { signed(0x4016_cbe4) } else { signed(0x3f49_0fdb) };
    }
    let (big, small) = if ax < ay { (ay, ax) } else { (ax, ay) };
    let mut r = small / big;
    if r <= 0.0625 {
        if r >= f64::from_bits(0x3f1a_36e2_eb1c_432d) {
            let r2 = r * r;
            r -= ((f64::from_bits(0x3fd5_5555_5555_5538) - (f64::from_bits(0x3fc9_9999_9996_43a3) - r2 * f64::from_bits(0x3fc2_4924_82bd_6be1)) * r2) * r2) * r;
        }
    } else {
        let j = (r * 256.0 + 0.5) as u32;
        let jf = j as f64;
        let t = (small * 256.0 - jf * big) / (jf * small + big * 256.0);
        r = (t + f64::from_bits(ATAN_JBY256[(j - 16) as usize])) - ((t * t) * t) * f64::from_bits(0x3fd5_5555_5555_0877);
    }
    if ax < ay {
        r = std::f64::consts::FRAC_PI_2 - r;
    }
    if x_neg {
        r = std::f64::consts::PI - r;
    }
    if y_neg {
        r = -r;
    }
    r as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acosf_within_an_ulp() {
        for i in -1000..=1000 {
            let x = i as f32 / 1000.0;
            let (ours, exact) = (acosf(x), (x as f64).acos() as f32);
            assert!((ours.to_bits() as i32 - exact.to_bits() as i32).abs() <= 1, "acosf({x}) = {ours} vs {exact}");
        }
    }

    #[test]
    fn random_follows_the_engine_xorshift() {
        let mut r = Random::new(1);
        let first = r.state;
        assert_eq!(first, [1, 0x6c078966, 0x6c078966u32.wrapping_mul(0x6c078965).wrapping_add(1), r.state[3]]);
        let t = 1u32 << 11 ^ 1;
        let w = first[3];
        assert_eq!(r.next(), ((w >> 11 ^ t) >> 8) ^ w ^ t);
        assert_eq!(r.state[..3], first[1..]);
        assert_eq!(Random::new(5).range_int(3, 3), 3);
    }
}
