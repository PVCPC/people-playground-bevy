//! JetEngineBehaviour (the Jet Engine): Use toggles it; the startup and shutoff coroutines ramp the intake and exhaust
//! PointEffector2Ds, the exhaust heats what it overlaps, and the Active state pushes the body along its right vector.
//! Awake only caches components and clears the plasma sprite's colour, and SetVisualJetIntensity sets the plasma and
//! light sprites' colours (StartupIntensityCurve only feeds it), the AudioSource calls play sounds: all rendering and
//! audio. OnAfterDeserialise belongs to save loading, which the rewrite does not do.
#![allow(non_snake_case)]

use super::coroutine::{Routine, Yield};
use super::translated::TBehaviour;
use super::{d, mathf, script, Game};
use crate::b2::world::FixtureId;

/// JetEngineBehaviour.State
const IDLE: i32 = 0;
const STARTING: i32 = 1;
const ACTIVE: i32 = 2;
const SHUTDOWN: i32 = 3;

script! {
    JetEngine {
        IntakeForceMultiplier: f32 = 1.0,
        ExhaustForceMultiplier: f32 = 1.0,
        ExhaustTemperature: f32 = 800.0,
        StartupDurationSeconds: f32 = 11.0,
        ShutoffDurationSeconds: f32 = 1.0,
        ScreenShakeIntensity: f32 = 0.25,
        WillDrown: bool = true,
        Activated: bool = false,
        CurrentState: i32 = 0,
        Force: f32 = 0.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// PhysicalBehaviour: its body, Transform and IsUnderWater.
        PhysicalBehaviour: usize = 0,
        /// DamagableMachinery
        DamagableMachinery: usize = 0,
        /// IntakeEffector and ExhaustEffector: the PointEffector2D component ids.
        IntakeEffector: i32 = 0,
        ExhaustEffector: i32 = 0,
        /// exhaustEffectorCollider: the fixtures of the first Collider2D on the ExhaustEffector's GameObject.
        exhaustEffectorCollider: Vec<FixtureId> = Vec::new(),
        /// guideTemperatureBehaviour: GetComponent<GuideTemperatureBehaviour>() (a translated behaviour); None is null.
        guideTemperatureBehaviour: Option<TBehaviour> = None,
    }
}

impl Game {
    fn jet_set_exhaust(&mut self, j: usize, v: f32) {
        self.effector_set_force_magnitude(self.jet_engines[j].ExhaustEffector, v);
    }

    fn jet_set_intake(&mut self, j: usize, v: f32) {
        self.effector_set_force_magnitude(self.jet_engines[j].IntakeEffector, v);
    }

    /// guideTemperatureBehaviour.enabled = v (Behaviour::SetEnabled: AddToManager/RemoveFromManager on an active
    /// GameObject; GuideTemperatureBehaviour has no OnEnable or OnDisable).
    fn jet_set_guide_enabled(&mut self, j: usize, v: bool) {
        let Some(TBehaviour::GuideTemperatureBehaviour(g)) = self.jet_engines[j].guideTemperatureBehaviour else {
            unimplemented!("JetEngineBehaviour: guideTemperatureBehaviour is null (NullReferenceException)");
        };
        if std::mem::replace(&mut self.tr.guide_temperature_behaviour[g].enabled, v) == v {
            return;
        }
        let id = self.tr.guide_temperature_behaviour[g].this_id;
        if self.with_component_instance(id, |_, inst, o| inst.objects[o].active) {
            let (order, b) = self.behaviour_of[&id];
            if v {
                self.behaviours.add(order, b);
            } else {
                self.behaviours.remove(b);
            }
        }
    }

    /// base.transform.localScale.x and base.transform.position (the jet's PhysicalBehaviour is on its GameObject).
    fn jet_transform(&self, j: usize) -> (f32, [f32; 2]) {
        let t = &self.physicals[self.jet_engines[j].PhysicalBehaviour].transform;
        (t.localScale[0], [t.position[0], t.position[1]])
    }

    /// PhysicalBehaviour.rigidbody.AddRelativeForce(new Vector2(x, 0), ForceMode2D.Force)
    fn jet_add_relative_force(&mut self, j: usize, x: f32) {
        let rb = self.physicals[self.jet_engines[j].PhysicalBehaviour].rigidbody.clone();
        rb.add_relative_force(&mut self.world, [x, 0.0]);
    }

    /// JetEngineBehaviour.FixedUpdate
    pub fn jet_engine_fixed_update(&mut self, j: usize) {
        match self.jet_engines[j].CurrentState {
            IDLE | SHUTDOWN => {
                self.jet_set_exhaust(j, 0.0);
                self.jet_set_intake(j, 0.0);
            }
            STARTING => self.jet_drown(j),
            ACTIVE => {
                let (scale, position) = self.jet_transform(j);
                let num = scale.abs();
                let s = &self.jet_engines[j];
                let (exhaust, intake, force, shake) = (s.ExhaustForceMultiplier * num, s.IntakeForceMultiplier * num, s.Force * scale, s.ScreenShakeIntensity);
                self.jet_set_exhaust(j, exhaust);
                self.jet_set_intake(j, intake);
                self.jet_exhaust_affect_objects(j, 1.0);
                self.jet_add_relative_force(j, force);
                self.camera_shake(shake, position);
                self.jet_drown(j);
            }
            _ => {}
        }
    }

    /// `if (WillDrown && PhysicalBehaviour.IsUnderWater) DamagableMachinery.ForceBreak();`
    fn jet_drown(&mut self, j: usize) {
        let s = &self.jet_engines[j];
        if s.WillDrown && self.physicals[s.PhysicalBehaviour].IsUnderWater {
            self.machinery_force_break(s.DamagableMachinery);
        }
    }

    /// JetEngineBehaviour.ExhaustAffectObjects: Physics2D.OverlapCollider with a ContactFilter2D that sets only the layer
    /// mask (useLayerMask, useTriggers and the depth tests stay off) into the 32-entry buffer.
    fn jet_exhaust_affect_objects(&mut self, j: usize, intensity: f32) {
        let t = 0.05f32 * intensity;
        let fixtures = self.jet_engines[j].exhaustEffectorCollider.clone();
        let hits = self.overlap_collider(&fixtures, u32::MAX);
        if hits.len() > 32 {
            unimplemented!("JetEngineBehaviour.ExhaustAffectObjects: more overlapping colliders than the 32-entry buffer");
        }
        let (own, target) = (self.jet_engines[j].PhysicalBehaviour, self.jet_engines[j].ExhaustTemperature);
        for (physical, _) in hits {
            // collider.gameObject != base.gameObject, and PhysicalObjectsInWorldByTransform.TryGetValue(collider.transform).
            let Some(p) = physical.filter(|&p| p != own) else { continue };
            if self.physicals[p].SimulateTemperature && self.physicals[p].Temperature < target {
                self.physicals[p].Temperature = mathf::lerp(self.physicals[p].Temperature, target, t);
            }
        }
    }

    /// StartCoroutine on the jet engine's own GameObject.
    fn jet_start_coroutine(&mut self, j: usize, r: Routine) {
        let id = self.jet_engines[j].id;
        self.start_coroutine(id, id, r);
    }

    /// JetEngineBehaviour.StartupRoutine
    fn jet_startup_routine(j: usize) -> Routine {
        let mut started = false;
        let mut t = 0.0f32;
        Box::new(move |g| {
            if !started {
                started = true;
                if g.jet_engines[j].CurrentState != IDLE {
                    return Yield::Done;
                }
                g.jet_engines[j].CurrentState = STARTING;
                t = 0.0;
                // Source.PlayOneShot(StartupSound) and SetVisualJetIntensity(0): audio and rendering.
                g.jet_set_guide_enabled(j, true);
            }
            let s = &g.jet_engines[j];
            if t < s.StartupDurationSeconds {
                let num = t / s.StartupDurationSeconds;
                let num2 = (d(num) * num * 0.25f32).f();
                let (scale, position) = g.jet_transform(j);
                let num3 = num2 * scale.abs();
                let s = &g.jet_engines[j];
                let (exhaust, intake) = (s.ExhaustForceMultiplier * num3, s.IntakeForceMultiplier * num3);
                let (shake, force) = ((d(num) * s.ScreenShakeIntensity * 0.5f32).f(), (d(s.Force) * scale * num * 0.4f32).f());
                g.jet_set_exhaust(j, exhaust);
                g.jet_set_intake(j, intake);
                g.jet_exhaust_affect_objects(j, num2);
                // SetVisualJetIntensity(StartupIntensityCurve.Evaluate(num)) is rendering.
                g.camera_shake(shake, position);
                g.jet_add_relative_force(j, force);
                t += g.time.deltaTime;
                return Yield::WaitForFixedUpdate;
            }
            // SetVisualJetIntensity(1f) and Source.Play() are rendering and audio.
            let (_, position) = g.jet_transform(j);
            let shake = 40.0f32 * g.jet_engines[j].ScreenShakeIntensity;
            g.camera_shake(shake, position);
            g.jet_engines[j].CurrentState = ACTIVE;
            Yield::Done
        })
    }

    /// JetEngineBehaviour.ShutoffRoutine
    fn jet_shutoff_routine(j: usize) -> Routine {
        let mut started = false;
        let mut waiting = false;
        let mut t = 0.0f32;
        Box::new(move |g| {
            if !started {
                started = true;
                if g.jet_engines[j].CurrentState != ACTIVE {
                    return Yield::Done;
                }
                g.jet_engines[j].CurrentState = SHUTDOWN;
                t = 0.0;
                // Source.Stop(), Source.PlayOneShot(ShutoffSound) and SetVisualJetIntensity(1f): audio and rendering.
                g.jet_set_guide_enabled(j, false);
            }
            if waiting {
                g.jet_engines[j].CurrentState = IDLE;
                return Yield::Done;
            }
            if t < g.jet_engines[j].ShutoffDurationSeconds {
                // SetVisualJetIntensity(1f - t / ShutoffDurationSeconds) is rendering.
                t += g.time.deltaTime;
                return Yield::WaitForEndOfFrame;
            }
            g.jet_set_exhaust(j, 0.0);
            g.jet_set_intake(j, 0.0);
            waiting = true;
            Yield::WaitForSeconds(4.0)
        })
    }

    /// JetEngineBehaviour.Use
    pub fn jet_engine_use(&mut self, j: usize) {
        if self.machinery[self.jet_engines[j].DamagableMachinery].Destroyed {
            return;
        }
        let activated = !self.jet_engines[j].Activated;
        self.jet_engines[j].Activated = activated;
        if activated {
            // TryStartup
            if !self.machinery[self.jet_engines[j].DamagableMachinery].Destroyed {
                self.jet_start_coroutine(j, Self::jet_startup_routine(j));
            }
        } else {
            // TryShutoff
            self.jet_start_coroutine(j, Self::jet_shutoff_routine(j));
        }
    }

    /// JetEngineBehaviour.ForceShutoff (DamagableMachineryBehaviour.OnDamaged's call).
    pub fn jet_engine_force_shutoff(&mut self, id: i32) {
        let j = self.jet_engines.iter().position(|x| x.id == id).expect("a JetEngineBehaviour in the arena");
        self.stop_all_coroutines(id);
        self.jet_engines[j].Activated = false;
        self.jet_set_exhaust(j, 0.0);
        self.jet_set_exhaust(j, 0.0);
        self.jet_set_intake(j, 0.0);
        self.jet_engines[j].CurrentState = IDLE;
        // Source.Stop() and SetVisualJetIntensity(0f): audio and rendering.
        self.jet_set_guide_enabled(j, false);
    }
}
