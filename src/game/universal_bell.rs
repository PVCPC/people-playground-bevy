//! UniversalBellBehaviour (the piano-note bell). Its clapper swings on the bell's own acceleration (FixedUpdate) and on
//! gravity (Update); a clapper that reaches +-24 degrees bounces, which changes the body's angular velocity. Use (Play)
//! kicks the clapper and torques the body. Start's SetBellIndex sets the physical's impact and shot sound arrays to the
//! note's one clip, whose lengths the draws count. Colours, labels, context menus and the audio are presentation.
#![allow(non_snake_case)]

use super::{mathf, script, BareAnchor, Game};
use crate::unity::transform::Trs;

script! {
    UniversalBell {
        ClapperVelocityInfluence: f32 = 100.0,
        ClapperIntensity: f32 = 0.0,
    }
    links {
        /// The component's instance id and enabled (Update and FixedUpdate run only while it is).
        id: i32 = 0,
        enabled: bool = true,
        /// PhysicalBehaviour (GetComponent in Start).
        physical: usize = 0,
        /// The clapper's angle (degrees) and its angular velocity; the private fields of the same names.
        clapperRot: f32 = 0.0,
        clapperRotVel: f32 = 0.0,
        /// previousVelocity: the body's velocity at the last FixedUpdate.
        previousVelocity: [f32; 2] = [0.0; 2],
    }
}

impl Game {
    /// UniversalBellBehaviour.Start: SetBellIndex makes the physical's OverrideImpactSounds one clip long (it is
    /// reused when it already is) and OverrideShotSounds the same array.
    pub fn universal_bell_start(&mut self, b: usize) {
        let p = self.universal_bells[b].physical;
        self.physicals[p].OverrideImpactSounds = 1;
        self.physicals[p].OverrideShotSounds = 1;
    }

    /// UniversalBellBehaviour.FixedUpdate: the body's velocity change, in the bell's local x, kicks the clapper.
    pub fn universal_bell_fixed_update(&mut self, b: usize) {
        let p = self.universal_bells[b].physical;
        let rb = self.physicals[p].rigidbody.clone();
        let velocity = rb.velocity(&self.world);
        let previous = self.universal_bells[b].previousVelocity;
        let delta = [velocity[0] - previous[0], velocity[1] - previous[1], 0.0];
        // base.transform.InverseTransformVector(delta).x
        let local = self.anchored_inverse_vector(BareAnchor::Physical(p), &[], delta);
        let s = &mut self.universal_bells[b];
        s.clapperRotVel -= local[0] * s.ClapperVelocityInfluence;
        s.previousVelocity = velocity;
    }

    /// UniversalBellBehaviour.Update: the clapper's velocity eases, gravity swings it, and it moves.
    pub fn universal_bell_update(&mut self, b: usize) {
        let p = self.universal_bells[b].physical;
        let dt = self.time.deltaTime;
        let eased = mathf::lerp_factor_dt(0.9, dt);
        {
            let s = &mut self.universal_bells[b];
            s.clapperRotVel = mathf::lerp(s.clapperRotVel, 0.0, eased);
        }
        let rot = self.universal_bells[b].clapperRot;
        if rot < -24.0 {
            self.universal_bells[b].clapperRot = -24.0;
            self.universal_bell_bounce(b);
        } else if rot > 24.0 {
            self.universal_bells[b].clapperRot = 24.0;
            self.universal_bell_bounce(b);
        }
        let rb = self.physicals[p].rigidbody.clone();
        let g = [self.world.gravity.x, self.world.gravity.y];
        // Physics2D.gravity.magnitude * rigidbody.gravityScale
        let num = mathf::magnitude2(g) * rb.gravity_scale(&self.world);
        if num > f32::EPSILON {
            let vector = [g[0] / num, g[1] / num];
            // base.transform.InverseTransformDirection(vector): the inverse rotations only, no scale.
            let v3 = self.anchored_inverse_direction(p, [vector[0], vector[1], 0.0]);
            let angle = ((v3[0] as f64).atan2((0.0 - v3[1]) as f64) as f32) * 57.29578;
            let clamped = mathf::clamp(angle - self.universal_bells[b].clapperRot, -90.0, 90.0);
            // Vector2.Dot(v, Vector2.down) = v.x * 0 + v.y * -1
            let dot = v3[0] * 0.0 + v3[1] * -1.0;
            let weight = mathf::max(dot, 0.05);
            let time_scale = self.time.timeScale;
            let s = &mut self.universal_bells[b];
            s.clapperRotVel += num / 9.81 * time_scale * clamped * weight;
        }
        let s = &mut self.universal_bells[b];
        s.clapperRot += s.clapperRotVel * dt;
    }

    /// The bounce: the clapper's velocity takes the body's spin and reverses, and a Dynamic body's spin loses 10%.
    fn universal_bell_bounce(&mut self, b: usize) {
        let p = self.universal_bells[b].physical;
        let rb = self.physicals[p].rigidbody.clone();
        let spin = rb.angular_velocity(&self.world);
        {
            let s = &mut self.universal_bells[b];
            s.clapperRotVel = mathf::lerp(s.clapperRotVel, spin, 0.2);
            s.clapperRotVel *= -0.25;
        }
        if rb.bodyType == 0 {
            let spin = rb.angular_velocity(&self.world);
            rb.set_angular_velocity(&mut self.world, spin * 0.9);
        }
    }

    /// Transform.InverseTransformDirection of physical `p`'s object: each node's inverse rotation, root first.
    fn anchored_inverse_direction(&self, p: usize, v: [f32; 3]) -> [f32; 3] {
        let mut v = [v[0], v[1], v[2], 0.0];
        for node in self.anchored_nodes(BareAnchor::Physical(p), &[]).iter().rev() {
            v = crate::unity::transform::inverse_vector(&Trs { s: [1.0; 4], ..*node }, v);
        }
        [v[0], v[1], v[2]]
    }

    /// UniversalBellBehaviour.Use: Play: the clapper kicks by ClapperIntensity and the body takes 0.005 of it as torque.
    pub fn universal_bell_use(&mut self, b: usize) {
        let i = self.universal_bells[b].ClapperIntensity;
        self.universal_bells[b].clapperRotVel += i;
        let p = self.universal_bells[b].physical;
        let rb = self.physicals[p].rigidbody.clone();
        rb.add_torque(&mut self.world, i * 0.005);
    }
}
