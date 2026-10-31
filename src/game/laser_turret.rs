//! LaserTurretBehaviour (the laser sentry turret). FixedUpdate aims the barrel with the AutomaticSentryController and sets the
//! joint motor to that speed times motorSpeed while it is not broken; the controller's OnShoot calls Shoot. Shoot (CanShoot)
//! waits out the cooldown (less the body's charge), pushes the body back by the recoil at the barrel, and instantiates the
//! bolt, turned to the barrel direction plus an inaccuracy draw. Use shoots. OnEMPHit breaks it (SetAsBroken): the motor
//! stops. The camera shake, the sounds, the muzzle flash, the glow and the break effect are presentation.
#![allow(non_snake_case)]

use super::automatic_sentry::{AutomaticSentry, SentryEvent};
use super::translated::rt::Xf;
use super::{script, Game};
use crate::assets::db::ObjRef;

script! {
    LaserTurret {
        radius: f32 = 20.0,
        recoil: f32 = 5.0,
        cooldown: f32 = 0.0,
        inaccuracy: f32 = 0.05,
        motorSpeed: f32 = 3.0,
        barrelPosition: [f32; 2] = [0.0; 2],
        barrelDirection: [f32; 2] = [1.0, 0.0],
        IsBroken: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// physicalBehaviour: the PhysicalBehaviour (`Game::physicals` index).
        physical: usize = 0,
        /// joint: the HingeJoint2D (`Game::hinges` index).
        joint: usize = 0,
        /// layerMask.m_Bits
        LayerMask: u32 = 0,
        /// projectile: the bolt prefab (None: Instantiate(null) throws when it fires).
        projectile: Option<ObjRef> = None,
        /// The time of the last shot (lastTimeShot).
        lastTimeShot: f32 = 0.0,
        sentry: AutomaticSentry = AutomaticSentry::default(),
    }
}

impl Game {
    /// BarrelPosition: transform.TransformPoint(barrelPosition), as a Vector2.
    fn laser_turret_barrel_position(&self, l: usize) -> [f32; 2] {
        let t = &self.laser_turrets[l];
        let p = self.transform_point3(t.physical, [t.barrelPosition[0], t.barrelPosition[1], 0.0]);
        [p[0], p[1]]
    }

    /// BarrelDirection: transform.TransformDirection(barrelDirection) * transform.localScale.x.
    fn laser_turret_barrel_direction(&self, l: usize) -> [f32; 2] {
        let t = &self.laser_turrets[l];
        let v = self.transform_direction(t.physical, [t.barrelDirection[0], t.barrelDirection[1], 0.0]);
        let s = self.physicals[t.physical].transform.localScale[0];
        [v[0] * s, v[1] * s]
    }

    /// LaserTurretBehaviour.Start: the sentry's sight range and mask; a broken one breaks again (SetAsBroken).
    pub fn laser_turret_start(&mut self, l: usize) {
        let t = &mut self.laser_turrets[l];
        t.sentry.SightRange = t.radius;
        t.sentry.LayerMask = t.LayerMask;
        if self.laser_turrets[l].IsBroken {
            self.laser_turret_set_as_broken(l);
        }
    }

    /// LaserTurretBehaviour.SetAsBroken: the break effect and glow are presentation; the motor stops.
    pub fn laser_turret_set_as_broken(&mut self, l: usize) {
        let h = self.laser_turrets[l].joint;
        if self.hinges[h].destroyed {
            unimplemented!("LaserTurretBehaviour.Joint destroyed (MissingReferenceException in SetAsBroken)");
        }
        self.hinges[h].set_use_motor(&mut self.world, false);
        self.laser_turrets[l].IsBroken = true;
    }

    /// LaserTurretBehaviour.Use: Shoot unless broken.
    pub fn laser_turret_use(&mut self, l: usize) {
        if !self.laser_turrets[l].IsBroken {
            self.laser_turret_shoot(l);
        }
    }

    /// LaserTurretBehaviour.FixedUpdate: the sentry's motor speed (its OnShoot calls Shoot) drives the joint's motor.
    pub fn laser_turret_fixed_update(&mut self, l: usize) {
        if self.laser_turrets[l].IsBroken {
            return;
        }
        let (pos, dir) = (self.laser_turret_barrel_position(l), self.laser_turret_barrel_direction(l));
        let p = self.laser_turrets[l].physical;
        let mut sentry = std::mem::take(&mut self.laser_turrets[l].sentry);
        let delta = self.time.deltaTime;
        let (num, events) = self.sentry_get_target_motor_speed(&mut sentry, p, pos, dir, delta);
        self.laser_turrets[l].sentry = sentry;
        for event in events {
            match event {
                // The sight listener is audio (presentation).
                SentryEvent::Sight => {}
                SentryEvent::Shoot => self.laser_turret_shoot(l),
            }
        }
        let h = self.laser_turrets[l].joint;
        if self.hinges[h].destroyed {
            unimplemented!("LaserTurretBehaviour.Joint destroyed (MissingReferenceException in FixedUpdate)");
        }
        let speed = num * self.laser_turrets[l].motorSpeed;
        let torque = self.hinges[h].motor[1];
        self.hinges[h].set_motor(&mut self.world, speed, torque);
    }

    /// LaserTurretBehaviour.Shoot (the CanShoot override): the cooldown, the recoil on the body at the barrel, and the
    /// bolt clone turned to the barrel direction plus an inaccuracy draw (camera shake, sounds and the flash are presentation).
    pub fn laser_turret_shoot(&mut self, l: usize) {
        let time = self.time.time;
        let p = self.laser_turrets[l].physical;
        let charge = self.physicals[p].Charge();
        let (cooldown, last) = (self.laser_turrets[l].cooldown, self.laser_turrets[l].lastTimeShot);
        if !(time - last < cooldown - charge / 100.0) {
            self.laser_turrets[l].lastTimeShot = time;
            let pos = self.laser_turret_barrel_position(l);
            let dir = self.laser_turret_barrel_direction(l);
            let recoil = self.laser_turrets[l].recoil;
            let rb = self.physicals[p].rigidbody.clone();
            rb.add_force_at_position(&mut self.world, [dir[0] * (0.0 - recoil), dir[1] * (0.0 - recoil)], pos);
            let projectile = self.laser_turrets[l].projectile.unwrap_or_else(|| unimplemented!("LaserTurretBehaviour: Instantiate(null projectile) (ArgumentException)"));
            let id = self.spawn_prefab(projectile, [pos[0], pos[1], 0.0]);
            let r = self.random.inside_unit_circle();
            let inaccuracy = self.laser_turrets[l].inaccuracy;
            self.tr_set_right(Xf::Object(id), [dir[0] + inaccuracy * r[0], dir[1] + inaccuracy * r[1], 0.0]);
        }
    }
}
