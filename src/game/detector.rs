//! DetectorBehaviour with Utils.GetLaserEndPoint and MaterialAwareRaycast. The beam (LineRenderer positions), the glow
//! and the context menu are rendering and UI. IsolatedActivation and IsolatedContinuousActivation reach only the
//! components on the detector's own GameObject, and only a UseWireBehaviour receives them (link.rs refuses one).
#![allow(non_snake_case)]

use super::explosion::RayHit2D;
use super::{d, mathf, script, Game};
use crate::b2::math::Vec2 as V;

script! {
    Detector {
        RelativeVelocityThreshold: f32 = 0.0,
        HasToBeAlive: bool = false,
        HasToBeOnFire: bool = false,
        TriggerOnExit: bool = true,
        Range: f32 = 5.0,
        Activated: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// PhysicalBehaviour
        Phys: usize = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// LayersToDetect (LayerMask bits)
        LayersToDetect: u32 = 0,
        /// PropertyFilter, as PhysicalProperties arena indices
        PropertyFilter: Vec<usize> = Vec::new(),
        isDetecting: bool = false,
    }
}

/// Utils.LaserHit
#[derive(Clone, Copy, Debug)]
struct LaserHit {
    hit: Option<RayHit2D>,
    physical: Option<usize>,
    point: [f32; 2],
    totalDistance: f32,
}

const MAX_STEPS: i32 = 8;
const MAX_ITERATIONS: usize = 64;

impl Game {
    /// DetectorBehaviour.Awake (the property block and the ContinuousActivationBehaviour listener, which only sends
    /// IsolatedContinuousActivation, are rendering and unreceived).
    pub fn detector_awake(&mut self, i: usize) {
        self.detectors[i].Activated = true;
    }

    /// DetectorBehaviour.Start: UpdateActivated (the Line and the context menu are rendering and UI).
    pub fn detector_start(&mut self, i: usize) {
        self.detector_fixed_update(i);
    }

    /// DetectorBehaviour.Use
    pub fn detector_use(&mut self, i: usize) {
        if self.detectors[i].enabled {
            let s = &mut self.detectors[i];
            s.Activated = !s.Activated;
            // UpdateActivated: FixedUpdate, then the Line and EnabledLight toggles.
            self.detector_fixed_update(i);
        }
    }

    /// DetectorBehaviour.OnDisable (the Line, glow and EnabledLight are rendering)
    pub fn detector_on_disable(&mut self, i: usize) {
        self.detectors[i].Activated = false;
    }

    /// DetectorBehaviour.FixedUpdate
    pub fn detector_fixed_update(&mut self, i: usize) {
        if !self.detectors[i].Activated {
            self.detectors[i].isDetecting = false;
            return;
        }
        let s = &self.detectors[i];
        let p = s.Phys;
        let maximum_distance = self.physicals[p].Charge() + s.Range;
        let layers = s.LayersToDetect;
        // rigidbody.position and rigidbody.GetRelativeVector(Vector3.right) * (IsFlipped ? -1 : 1)
        self.sync_pending();
        let xf = self.world.body(self.physicals[p].rigidbody.body).xf;
        let sign = if self.physicals[p].transform.localScale[0] < 0.0 { -1.0 } else { 1.0 };
        let dir = [xf.q.c * sign, xf.q.s * sign];
        let mut hits = Vec::new();
        self.get_laser_end_point([xf.p.x, xf.p.y], dir, &mut hits, layers, maximum_distance);
        let fits = self.detector_fits_criteria(i, hits.last().copied());
        if !self.detectors[i].isDetecting || fits {
            return;
        }
        // TriggerOnExit: SendMessage("IsolatedActivation") on the detector's object, which nothing there receives.
        self.detectors[i].isDetecting = false;
    }

    /// DetectorBehaviour.FitsCriteria
    fn detector_fits_criteria(&mut self, i: usize, hit: Option<LaserHit>) -> bool {
        // LastOrDefault of an empty list is a default LaserHit: no PhysicalBehaviour.
        let Some(hit) = hit else { return false };
        let Some(p) = hit.physical else { return false };
        if hit.hit.is_none() {
            return false;
        }
        let s = &self.detectors[i];
        let (me, threshold, alive, fire) = (s.Phys, s.RelativeVelocityThreshold, s.HasToBeAlive, s.HasToBeOnFire);
        if alive {
            // AliveByTransform.TryGetValue(value.transform.root)
            let Some(person) = self.person_of_root(p) else { return false };
            if !self.person_is_alive(person) {
                return false;
            }
        }
        if threshold > 0.01 {
            self.sync_pending();
            let a = self.physicals[me].rigidbody.point_velocity(&self.world, hit.point);
            let b = self.physicals[p].rigidbody.point_velocity(&self.world, hit.point);
            if mathf::sqr_magnitude([a[0] - b[0], a[1] - b[1]]) < threshold {
                return false;
            }
        }
        if fire && !self.physicals[p].OnFire {
            return false;
        }
        let filter = &self.detectors[i].PropertyFilter;
        if !filter.is_empty() && !filter.contains(&self.physicals[p].Properties) {
            return false;
        }
        // !isDetecting: SendMessage("IsolatedActivation"), unreceived.
        self.detectors[i].isDetecting = true;
        true
    }

    /// Utils.MaterialAwareRaycast
    fn material_aware_raycast(&mut self, origin: [f32; 2], dir: [f32; 2], max_distance: f32, layers: u32, max_steps: i32, mut current_distance: f32) -> LaserHit {
        if max_steps <= 0 || max_distance <= 0.0 {
            return LaserHit { hit: None, physical: None, point: origin, totalDistance: 0.0 };
        }
        let Some(hit) = self.raycast(V::new(origin[0], origin[1]), dir, max_distance, layers, f32::NEG_INFINITY, f32::INFINITY) else {
            current_distance += max_distance;
            let point = [(d(dir[0]) * max_distance + origin[0]).f(), (d(dir[1]) * max_distance + origin[1]).f()];
            return LaserHit { hit: None, physical: None, point, totalDistance: current_distance };
        };
        let point = [hit.point.x, hit.point.y];
        // PhysicalObjectsInWorldByTransform.TryGetValue(raycastHit2D.transform)
        let Some(p) = self.physical_of_collider(hit.collider).filter(|&p| self.physicals[p].in_world) else {
            return LaserHit { hit: Some(hit), physical: None, point, totalDistance: current_distance + hit.distance };
        };
        if self.physicals[p].AbsorbsLasers {
            return LaserHit { hit: Some(hit), physical: Some(p), point, totalDistance: current_distance + hit.distance };
        }
        let next = [(d(dir[0]) * 0.01f32 + point[0]).f(), (d(dir[1]) * 0.01f32 + point[1]).f()];
        self.material_aware_raycast(next, dir, max_distance - hit.distance, layers, max_steps - 1, current_distance + hit.distance)
    }

    /// Utils.GetLaserEndPoint
    fn get_laser_end_point(&mut self, origin: [f32; 2], dir: [f32; 2], list: &mut Vec<LaserHit>, layers: u32, maximum_distance: f32) {
        if list.len() >= MAX_ITERATIONS {
            return;
        }
        let laser = self.material_aware_raycast(origin, dir, maximum_distance, layers, MAX_STEPS, 0.0);
        let add_to_list = |list: &mut Vec<LaserHit>| {
            let n = mathf::normalized2(dir);
            let point = [(d(n[0]) * maximum_distance + origin[0]).f(), (d(n[1]) * maximum_distance + origin[1]).f()];
            list.push(LaserHit { hit: None, physical: None, point, totalDistance: 0.0 });
        };
        let Some(value) = laser.hit else { return add_to_list(list) };
        let point = [value.point.x, value.point.y];
        let Some(p) = laser.physical else {
            list.push(LaserHit { hit: Some(value), physical: None, point, totalDistance: 0.0 });
            return;
        };
        // MaterialAwareRaycast only returns a PhysicalBehaviour that AbsorbsLasers.
        if !self.physicals[p].AbsorbsLasers {
            return add_to_list(list);
        }
        list.push(LaserHit { hit: Some(value), physical: Some(p), point, totalDistance: 0.0 });
        if self.physicals[p].ReflectsLasers {
            // Vector2.Reflect(dir, normal): -2 * Dot(normal, dir) scales the normal.
            let n = [value.normal.x, value.normal.y];
            let num = (d(-2.0) * (d(n[0]) * dir[0] + d(n[1]) * dir[1]).f()).f();
            let reflected = [(d(num) * n[0] + dir[0]).f(), (d(num) * n[1] + dir[1]).f()];
            self.get_laser_end_point(point, reflected, list, layers, maximum_distance - laser.totalDistance);
        }
    }
}
