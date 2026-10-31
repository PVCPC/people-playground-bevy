//! LaserBehaviour (the Laser Pointer and the Laser Attachment). Start and Use (when not an Attachment) run UpdateLaser:
//! the beam from the barrel through Utils.GetLaserEndPoint, which is detector.rs's get_laser_end_point. LateUpdate fires
//! while Activated: a hit physical gets heat, ignition, Slice and Break above charge thresholds, burn progress and, above
//! 700, an explosion every 0.12 s. OnLasered reaches only LaserReceiverBehaviour, which link refuses, so it does nothing.
//! The beam colours, line, particles and the context menu are presentation. The hoverbox egg branch needs ConnectedEggs,
//! which only EnergyWireBehaviour raises (not rewritten, link refuses it), so it is refused in laser_late_update.
#![allow(non_snake_case)]

use super::detector::LaserHit;
use super::explosion::{EffectSize, ExplosionParameters};
use super::{mathf, script, Game};

script! {
    Laser {
        Range: f32 = 5000.0,
        barrelPosition: [f32; 2] = [0.0, 0.0],
        barrelDirection: [f32; 2] = [1.0, 0.0],
        Activated: bool = false,
        Attachment: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// PhysicalBehaviour (the object's own)
        physical: usize = 0,
        /// LayersToHit (LayerMask bits)
        LayersToHit: u32 = 0,
        /// The explosion timer that the 700+ charge path counts.
        timer: f32 = 0.0,
    }
}

impl Game {
    /// LaserBehaviour.BarrelPosition: rigidbody.GetRelativePoint(barrelPosition * lossyScale).
    fn laser_barrel_position(&mut self, l: usize) -> [f32; 2] {
        let p = self.lasers[l].physical;
        let s = self.lossy_scale(p);
        let b = self.lasers[l].barrelPosition;
        let v = [b[0] * s[0], b[1] * s[1]];
        self.sync_pending();
        let xf = self.world.body(self.physicals[p].rigidbody.body).xf;
        [xf.q.c * v[0] - xf.q.s * v[1] + xf.p.x, xf.q.s * v[0] + xf.q.c * v[1] + xf.p.y]
    }

    /// LaserBehaviour.BarrelDirection: rigidbody.GetRelativeVector(barrelDirection), negated when lossyScale.x is negative.
    fn laser_barrel_direction(&mut self, l: usize) -> [f32; 2] {
        let p = self.lasers[l].physical;
        let b = self.lasers[l].barrelDirection;
        self.sync_pending();
        let xf = self.world.body(self.physicals[p].rigidbody.body).xf;
        let v = [xf.q.c * b[0] - xf.q.s * b[1], xf.q.s * b[0] + xf.q.c * b[1]];
        let sign = if self.lossy_scale(p)[0] < 0.0 { -1.0 } else { 1.0 };
        [v[0] * sign, v[1] * sign]
    }

    /// LaserBehaviour.UpdateLaser: the beam's hits from the barrel. Its result is the last hit's physical when there is
    /// one (`finalHit`, as the caller's Some), and the colours, line and particles it sets are presentation.
    fn laser_update(&mut self, l: usize) -> Option<(usize, LaserHit)> {
        let origin = self.laser_barrel_position(l);
        let direction = self.laser_barrel_direction(l);
        let (layers, range) = (self.lasers[l].LayersToHit, self.lasers[l].Range);
        let mut hits: Vec<LaserHit> = Vec::new();
        self.get_laser_end_point(origin, direction, &mut hits, layers, range, 64);
        let last = *hits.last()?;
        last.physical.map(|p| (p, last))
    }

    /// LaserBehaviour.UpdateActivation: UpdateLaser while Activated and enabled (the rest is presentation).
    pub(super) fn laser_update_activation(&mut self, l: usize) {
        if self.lasers[l].Activated && self.lasers[l].enabled {
            self.laser_update(l);
        }
    }

    /// LaserBehaviour.Start: the context menu and the line's world space are UI and presentation, then UpdateActivation.
    pub fn laser_start(&mut self, l: usize) {
        self.laser_update_activation(l);
    }

    /// LaserBehaviour.Use: `if (base.enabled && !Attachment)` Activated flips, then UpdateActivation.
    pub fn laser_use(&mut self, l: usize) {
        if self.lasers[l].enabled && !self.lasers[l].Attachment {
            self.lasers[l].Activated = !self.lasers[l].Activated;
            self.laser_update_activation(l);
        }
    }

    /// LaserBehaviour.FireLaser, for a laser that hit a physical. The charge is Mathf.Max(Charge, pseudoCharge) with
    /// pseudoCharge 0 (only the refused egg branch sets it). OnLasered reaches no receiver (see the module comment).
    fn laser_fire(&mut self, l: usize) {
        let Some((p, hit)) = self.laser_update(l) else { return };
        let dt = self.time.deltaTime;
        let num = mathf::max(self.physicals[self.lasers[l].physical].Charge(), 0.0);
        if num > 2.0 {
            let cap = self.heat_capacity(p);
            self.physicals[p].Temperature += dt * num / cap;
            self.ignite_at(p, false, hit.point);
            if num > 50.0 {
                self.send_slice(p);
                let collider = hit.hit.expect("a laser hit on a physical has a RayHit2D").collider;
                self.send_break(collider, Some(p), [0.0, 0.0]);
            }
            if num > 150.0 {
                self.physicals[p].BurnProgress = 1.0;
                if num > 700.0 {
                    self.lasers[l].timer += dt;
                    if self.lasers[l].timer > 0.12 {
                        let under_water = self.is_point_under_water(hit.point);
                        let ex = ExplosionParameters {
                            FragmentationRayCount: 6,
                            Position: [hit.point[0], hit.point[1], 0.0],
                            Range: 1.0,
                            FragmentForce: 3.0,
                            CreateParticlesAndSound: true,
                            EffectSize: EffectSize::Medium,
                            DismemberChance: 0.0,
                            BallisticShrapnelCount: 0,
                        };
                        self.create_explosion_with_water(under_water, ex);
                        self.lasers[l].timer = 0.0;
                    }
                }
            }
        }
        if num > 0.1 {
            self.physicals[p].BurnProgress += dt * num * 0.01;
        }
    }

    /// LaserBehaviour.LateUpdate: FireLaser while Activated. isFit is only cleared by OnAfterDeserialise (a loaded save,
    /// never rewritten), and ConnectedEggs stays 0 because EnergyWireBehaviour is not rewritten, so the hoverbox branch
    /// (Instantiate, AddForceAtPosition, Destroy) cannot run.
    pub fn laser_late_update(&mut self, l: usize) {
        if self.lasers[l].Activated {
            self.laser_fire(l);
        }
    }
}
