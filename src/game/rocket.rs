//! LaunchedRocketBehaviour (the Bazooka's and the Rocket Launcher's rocket): a Transform the script moves along its
//! wobbling right vector until a ray finds something, then the explosion and the Shot, ExitShot and Break messages.
//! The rocket has no physics of its own; its trail's Stop and the audio are presentation.
#![allow(non_snake_case)]

use super::explosion::ExplosionParameters;
use super::{mathf, script, Game};
use crate::b2::math::Vec2 as V;

/// LayerMask.GetMask("Objects", "Bounds")
const MASK: u32 = (1 << 9) | (1 << 11);

script! {
    LaunchedRocket {
        UnitsPerSecond: f32 = 1.0,
        AccelerationPerSecond: f32 = 0.5,
        MaxSpeed: f32 = 80.0,
        WaterSlowdown: f32 = 0.8,
        ImmobilityFieldSlowdown: f32 = 0.8,
        WobbleIntensity: f32 = 1.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// ImmobiltyFieldLayer's bits
        ImmobiltyFieldLayer: u32 = 0,
        /// relevantSystem: a component id on the trail's GameObject.
        relevantSystem: i32 = 0,
        Explosion: Option<ExplosionParameters> = None,
        Seed: f32 = 0.0,
        f: f32 = 0.0,
        speedMultiplier: f32 = 1.0,
    }
}

impl Game {
    /// LaunchedRocketBehaviour.Awake
    pub fn rocket_awake(&mut self, r: usize) {
        self.rockets[r].Seed = self.random.value() * 1000.0;
    }

    /// The rocket's transform.position.
    fn rocket_position(&mut self, r: usize) -> [f32; 3] {
        self.with_component_instance(self.rockets[r].id, |_, inst, o| inst.position(o))
    }

    /// LaunchedRocketBehaviour.FixedUpdate
    pub fn rocket_fixed_update(&mut self, r: usize) {
        let p = self.rocket_position(r);
        let s = &self.rockets[r];
        let inside = self.overlap_point_first([p[0], p[1]], s.ImmobiltyFieldLayer).is_some();
        let s = &mut self.rockets[r];
        s.speedMultiplier = if inside { s.speedMultiplier * s.ImmobilityFieldSlowdown } else { 1.0 };
    }

    /// LaunchedRocketBehaviour.Update
    pub fn rocket_update(&mut self, r: usize) {
        let id = self.rockets[r].id;
        let (position, rotation) = self.with_component_instance(id, |_, inst, o| (inst.position(o), inst.rotation(o)));
        let mut right = mathf::rotate(rotation, [1.0, 0.0, 0.0]);
        let s = &self.rockets[r];
        // float.Epsilon, the smallest denormal.
        if s.WobbleIntensity > f32::from_bits(1) {
            let t = self.time.time;
            let k = 0.25 * s.WobbleIntensity;
            let w = [mathf::perlin(0.0, 2.0 * t + s.Seed) * 2.0 - 1.0, mathf::perlin(2.0 * t - s.Seed, 5.0) * 2.0 - 1.0, 0.0];
            right = super::explosion::normalized3([right[0] + k * w[0], right[1] + k * w[1], right[2] + k * w[2]]);
        }
        let water = if self.is_point_under_water([position[0], position[1]]) { self.rockets[r].WaterSlowdown } else { 1.0 };
        let s = &self.rockets[r];
        let num = water * s.speedMultiplier;
        let num2 = mathf::min(s.MaxSpeed, (s.UnitsPerSecond + s.AccelerationPerSecond * s.f) * num) * self.time.deltaTime;
        match self.raycast(V::new(position[0], position[1]), [right[0], right[1]], num2, MASK, f32::NEG_INFINITY, f32::INFINITY) {
            Some(hit) => {
                let (point, normal) = ([hit.point.x, hit.point.y], [hit.normal.x, hit.normal.y]);
                let vector = [point[0] + normal[0] * 0.05, point[1] + normal[1] * 0.05];
                // relevantSystem.transform.SetParent(null); Stop is presentation; AddComponent<DeleteAfterTime>().Life = 5f.
                let trail = self.rockets[r].relevantSystem;
                self.set_parent_null_of(trail, |_, _, _| {});
                let key = self.with_component_instance(trail, |_, inst, o| inst.objects[o].components[0]);
                self.add_delete_after(key, 5.0);
                self.destroy_object(id);
                let mut ex = self.rockets[r].Explosion.clone().expect("LaunchedRocketBehaviour.Explosion");
                ex.Position = [vector[0], vector[1], 0.0];
                self.explosion_creator_explode(ex);
                let p = self.hit_transform_physical(&hit);
                self.send_shot(hit.collider, p, normal, point, 35.0, false, None);
                self.send_shot(hit.collider, p, normal, point, 35.0, true, None);
                self.send_break(hit.collider, p, [num2 * right[0], num2 * right[1]]);
            }
            None => {
                let to = [position[0] + right[0] * num2, position[1] + right[1] * num2, position[2] + right[2] * num2];
                self.with_component_instance(id, |_, inst, o| inst.set_world_position(o, to));
            }
        }
        self.rockets[r].f += self.time.deltaTime;
    }
}
