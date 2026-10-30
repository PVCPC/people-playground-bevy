//! Physics2DSettings (globalgamemanagers class 19) and the b2_ globals Physics2DSettings::UpdateBox2D (180c16280)
//! copies out of it. Unity's Box2D reads those globals everywhere, so they are process-wide here too.

use super::math::Vec2;
use crate::assets::classes::PPtr;
use crate::assets::serialized::Reader;
use std::sync::RwLock;

/// Physics2DSettings::UpdateBox2D's conversion of degrees (the float at 15e7a84).
pub const DEG_TO_RAD: f32 = f32::from_bits(0x3c8e_fa35);

/// PhysicsJobOptions2D: two flags, then the nine per-job counts and six island solver values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JobOptions {
    pub use_multithreading: bool,
    pub use_consistency_sorting: bool,
    /// InterpolationPosesPerJob, NewContactsPerJob, CollideContactsPerJob, ClearFlagsPerJob, ClearBodyForcesPerJob,
    /// SyncDiscreteFixturesPerJob, SyncContinuousFixturesPerJob, FindNearestContactsPerJob,
    /// UpdateTriggerContactsPerJob, IslandSolverCostThreshold, IslandSolverBodyCostScale,
    /// IslandSolverContactCostScale, IslandSolverJointCostScale, IslandSolverBodiesPerJob,
    /// IslandSolverContactsPerJob, as stored.
    pub words: [u32; 15],
}

/// The class 19 object, in the order Physics2DSettings::Transfer<StreamedBinaryRead> (180c09820) reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Physics2DSettings {
    pub gravity: Vec2,
    pub default_material: PPtr,
    pub velocity_iterations: i32,
    pub position_iterations: i32,
    pub velocity_threshold: f32,
    pub max_linear_correction: f32,
    pub max_angular_correction: f32,
    pub max_translation_speed: f32,
    pub max_rotation_speed: f32,
    pub baumgarte_scale: f32,
    pub baumgarte_toi_scale: f32,
    pub time_to_sleep: f32,
    pub linear_sleep_tolerance: f32,
    pub angular_sleep_tolerance: f32,
    pub default_contact_offset: f32,
    pub job_options: JobOptions,
    pub simulation_mode: i32,
    pub queries_hit_triggers: bool,
    pub queries_start_in_colliders: bool,
    pub callbacks_on_disable: bool,
    pub reuse_collision_callbacks: bool,
    pub auto_sync_transforms: bool,
    /// m_LayerCollisionMatrix: bit j of entry i set when layers i and j collide.
    pub layer_collision_matrix: Vec<u32>,
}

/// The b2_ globals UpdateBox2D writes: b2_polygonRadius (1999c28), b2_velocityThreshold, b2_maxLinearCorrection,
/// b2_maxAngularCorrection (radians), b2_maxTranslation and its square (1a43874), b2_maxRotation (radians) and its
/// square (1a43878), b2_baumgarte, b2_toiBaugarte, b2_timeToSleep, b2_linearSleepTolerance and
/// b2_angularSleepTolerance (radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Globals {
    pub polygon_radius: f32,
    pub velocity_threshold: f32,
    pub max_linear_correction: f32,
    pub max_angular_correction: f32,
    pub max_translation: f32,
    pub max_translation_squared: f32,
    pub max_rotation: f32,
    pub max_rotation_squared: f32,
    pub baumgarte: f32,
    pub toi_baumgarte: f32,
    pub time_to_sleep: f32,
    pub linear_sleep_tolerance: f32,
    pub angular_sleep_tolerance: f32,
}

/// What globalgamemanagers holds for People Playground until `Physics2DSettings::update_box2d` runs (the
/// `shipped_globals_match_data` test compares it with the data).
static GLOBALS: RwLock<Globals> = RwLock::new(Globals {
    polygon_radius: 0.01,
    velocity_threshold: 1.0,
    max_linear_correction: 16.0,
    max_angular_correction: 8.0 * DEG_TO_RAD,
    max_translation: 100.0,
    max_translation_squared: 100.0 * 100.0,
    max_rotation: 360.0 * DEG_TO_RAD,
    max_rotation_squared: 360.0 * DEG_TO_RAD * (360.0 * DEG_TO_RAD),
    baumgarte: 0.2,
    toi_baumgarte: 0.75,
    time_to_sleep: 0.5,
    linear_sleep_tolerance: 0.01,
    angular_sleep_tolerance: 2.0 * DEG_TO_RAD,
});

/// The current b2_ globals.
pub fn globals() -> Globals {
    *GLOBALS.read().unwrap()
}

impl Physics2DSettings {
    /// Transfer<StreamedBinaryRead>: Object has no serialized fields; the rest as listed in the struct.
    pub fn read(r: &mut Reader) -> Result<Self, String> {
        let gravity = Vec2::new(r.f32()?, r.f32()?);
        let default_material = PPtr::read(r)?;
        let (velocity_iterations, position_iterations) = (r.i32()?, r.i32()?);
        let mut f = || r.f32();
        let (velocity_threshold, max_linear_correction, max_angular_correction, max_translation_speed, max_rotation_speed) = (f()?, f()?, f()?, f()?, f()?);
        let (baumgarte_scale, baumgarte_toi_scale, time_to_sleep, linear_sleep_tolerance, angular_sleep_tolerance, default_contact_offset) = (f()?, f()?, f()?, f()?, f()?, f()?);
        let (use_multithreading, use_consistency_sorting) = (r.bool()?, r.bool()?);
        r.align();
        let mut words = [0; 15];
        for w in &mut words {
            *w = r.u32()?;
        }
        let simulation_mode = r.i32()?;
        let (queries_hit_triggers, queries_start_in_colliders, callbacks_on_disable, reuse_collision_callbacks, auto_sync_transforms) = (r.bool()?, r.bool()?, r.bool()?, r.bool()?, r.bool()?);
        r.align();
        let n = r.i32()?;
        let layer_collision_matrix = (0..n).map(|_| r.u32()).collect::<Result<_, _>>()?;
        Ok(Self {
            gravity,
            default_material,
            velocity_iterations,
            position_iterations,
            velocity_threshold,
            max_linear_correction,
            max_angular_correction,
            max_translation_speed,
            max_rotation_speed,
            baumgarte_scale,
            baumgarte_toi_scale,
            time_to_sleep,
            linear_sleep_tolerance,
            angular_sleep_tolerance,
            default_contact_offset,
            job_options: JobOptions { use_multithreading, use_consistency_sorting, words },
            simulation_mode,
            queries_hit_triggers,
            queries_start_in_colliders,
            callbacks_on_disable,
            reuse_collision_callbacks,
            auto_sync_transforms,
            layer_collision_matrix,
        })
    }

    /// The b2_ globals as UpdateBox2D computes them.
    pub fn globals(&self) -> Globals {
        let max_rotation = self.max_rotation_speed * DEG_TO_RAD;
        Globals {
            polygon_radius: self.default_contact_offset,
            velocity_threshold: self.velocity_threshold,
            max_linear_correction: self.max_linear_correction,
            max_angular_correction: self.max_angular_correction * DEG_TO_RAD,
            max_translation: self.max_translation_speed,
            max_translation_squared: self.max_translation_speed * self.max_translation_speed,
            max_rotation,
            max_rotation_squared: max_rotation * max_rotation,
            baumgarte: self.baumgarte_scale,
            toi_baumgarte: self.baumgarte_toi_scale,
            time_to_sleep: self.time_to_sleep,
            linear_sleep_tolerance: self.linear_sleep_tolerance,
            angular_sleep_tolerance: self.angular_sleep_tolerance * DEG_TO_RAD,
        }
    }

    /// Physics2DSettings::UpdateBox2D (180c16280).
    pub fn update_box2d(&self) {
        *GLOBALS.write().unwrap() = self.globals();
    }
}
