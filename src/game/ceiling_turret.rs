//! CeilingTurretBehaviour. OnEnable (before Start) turns the joint's motor on, and Start sets the sentry's sight range,
//! mask and aim fuzziness. Update asks the sentry for a motor speed: while it has a target that is the aim, else the
//! sweep (the joint angle towards a triangle wave between the limits, scaled down by the combat time). FixedUpdate sets
//! the motor to that speed times MotorSpeedMultiplier. A shot (the sentry's OnShoot) resets the combat time and fires
//! the firearm. OnSight and OnLoseTarget only play audio and set a renderer (presentation). OnDisable (the motor off)
//! runs only when the object is disabled, which the rewrite does not model.
#![allow(non_snake_case)]

use super::automatic_sentry::{AutomaticSentry, SentryEvent};
use super::{mathf, script, Game};

script! {
    CeilingTurret {
        SightRadius: f32 = 20.0,
        MotorSpeedMultiplier: f32 = 1.0,
        SweepAnimationSpeed: f32 = 4.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// The FirearmBehaviour's `Game::firearms` index.
        firearm: usize = 0,
        /// The Joint's `Game::hinges` index.
        joint: usize = 0,
        /// LayerMask.m_Bits
        LayerMask: u32 = 0,
        sentry: AutomaticSentry = AutomaticSentry::default(),
        targetMotorSpeed: f32 = 0.0,
        combatTime: f32 = 0.0,
        hadTarget: bool = false,
    }
}

/// Utils.Mod: a - floor(a / b) * b.
fn modulo(a: f32, b: f32) -> f32 {
    a - mathf::floor(a / b) * b
}

/// Utils.Triangle: a wave of period 2 pi with values in [-1, 1].
fn triangle(x: f32) -> f32 {
    let m = modulo(x / std::f32::consts::PI - 0.5, 2.0);
    (m - 1.0).abs() * 2.0 - 1.0
}

/// Mathf.LerpAngle(a, b, t): the short way round.
fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut delta = mathf::repeat(b - a, 360.0);
    if delta > 180.0 {
        delta -= 360.0;
    }
    a + delta * mathf::clamp01(t)
}

impl Game {
    /// CeilingTurretBehaviour.OnEnable: Joint.useMotor = true (its awake-time and SetActive arms in link.rs and water.rs).
    pub fn ceiling_turret_on_enable(&mut self, i: usize) {
        let h = self.ceiling_turrets[i].joint;
        if self.hinges[h].destroyed {
            unimplemented!("CeilingTurretBehaviour.Joint destroyed (MissingReferenceException in OnEnable)");
        }
        self.hinges[h].set_use_motor(&mut self.world, true);
    }

    /// CeilingTurretBehaviour.OnDisable: Joint.useMotor = false.
    pub fn ceiling_turret_on_disable(&mut self, i: usize) {
        let h = self.ceiling_turrets[i].joint;
        if self.hinges[h].destroyed {
            unimplemented!("CeilingTurretBehaviour.Joint destroyed (MissingReferenceException in OnDisable)");
        }
        self.hinges[h].set_use_motor(&mut self.world, false);
    }

    /// CeilingTurretBehaviour.Start: the sentry's sight range, mask and aim fuzziness.
    pub fn ceiling_turret_start(&mut self, i: usize) {
        let t = &mut self.ceiling_turrets[i];
        t.sentry.SightRange = t.SightRadius;
        t.sentry.LayerMask = t.LayerMask;
        t.sentry.AimFuzziness = 0.001;
    }

    /// CeilingTurretBehaviour.Update.
    pub fn ceiling_turret_update(&mut self, i: usize) {
        let (f, h) = (self.ceiling_turrets[i].firearm, self.ceiling_turrets[i].joint);
        let fp = self.firearms[f].physical.unwrap_or_else(|| unimplemented!("CeilingTurretBehaviour.FirearmBehaviour without a PhysicalBehaviour (its transform is read in Update)"));
        if self.hinges[h].destroyed {
            unimplemented!("CeilingTurretBehaviour.Joint destroyed (MissingReferenceException in Update)");
        }
        let (pos, dir) = (self.barrel_position(f), self.barrel_direction(f));
        let mut sentry = std::mem::take(&mut self.ceiling_turrets[i].sentry);
        let delta = self.time.deltaTime;
        let (speed, events) = self.sentry_get_target_motor_speed(&mut sentry, fp, pos, dir, delta);
        self.ceiling_turrets[i].sentry = sentry;
        self.ceiling_turrets[i].targetMotorSpeed = speed;
        for event in events {
            match event {
                // The sight listener sets hadTarget; OnSight.Invoke is presentation.
                SentryEvent::Sight => self.ceiling_turrets[i].hadTarget = true,
                // The shot listener: combatTime = 1, then the firearm's Shoot or ShootContinuous.
                SentryEvent::Shoot => {
                    self.ceiling_turrets[i].combatTime = 1.0;
                    if self.firearms[f].shotIndex == 0 {
                        self.firearm_shoot(f);
                    } else {
                        self.firearm_shoot_continuous(f);
                    }
                }
            }
        }
        if self.ceiling_turrets[i].sentry.currentTarget.is_none() {
            // `if (hadTarget) OnLoseTarget.Invoke()` is presentation (audio and a renderer).
            self.ceiling_turrets[i].hadTarget = false;
            let combat = self.ceiling_turrets[i].combatTime;
            let speed_sweep = self.ceiling_turrets[i].SweepAnimationSpeed;
            let [min, max] = self.hinges[h].limits;
            let angle = self.hinges[h].joint_angle(&self.world);
            let wave = triangle(self.time.time * speed_sweep) * 0.5 + 0.5;
            let target = lerp_angle(min, max, wave);
            self.ceiling_turrets[i].targetMotorSpeed = mathf::max(0.0, 1.0 - combat) * mathf::delta_angle(angle, target);
        } else {
            self.ceiling_turrets[i].hadTarget = true;
        }
    }

    /// CeilingTurretBehaviour.FixedUpdate: combatTime decays, and the motor takes the speed (its maxMotorTorque kept).
    pub fn ceiling_turret_fixed_update(&mut self, i: usize) {
        let h = self.ceiling_turrets[i].joint;
        if self.hinges[h].destroyed {
            unimplemented!("CeilingTurretBehaviour.Joint destroyed (MissingReferenceException in FixedUpdate)");
        }
        let t = &mut self.ceiling_turrets[i];
        t.combatTime *= 0.99;
        let speed = t.targetMotorSpeed * t.MotorSpeedMultiplier;
        let torque = self.hinges[h].motor[1];
        self.hinges[h].set_motor(&mut self.world, speed, torque);
    }
}
