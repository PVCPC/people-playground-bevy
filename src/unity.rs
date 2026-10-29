//! UnityEngine natives the game scripts call, rewritten from UnityPlayer.dll (Unity 2020.3).

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
        let (cos, sin) = (angle.cos(), angle.sin());
        let radius = self.range(0.0, 1.0).sqrt();
        [cos * radius, sin * radius]
    }

    /// RandomUnitVector(Rand&): Random.onUnitSphere
    pub fn on_unit_sphere(&mut self) -> [f32; 3] {
        let t = self.value();
        let z = (1.0 - t) - t;
        let angle = self.range(0.0, TWO_PI);
        let radius = (1.0 - z * z).sqrt();
        [angle.cos() * radius, angle.sin() * radius, z]
    }

    /// Random.insideUnitSphere
    pub fn inside_unit_sphere(&mut self) -> [f32; 3] {
        let v = self.on_unit_sphere();
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

#[cfg(test)]
mod tests {
    use super::*;

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
