//! Translated from FanBehaviour in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    FanBehaviour {
        Strength: f32 = 0.0,
        Range: f32 = 5.0,
        AreaWidth: f32 = 0.0,
        LocalStartPos: [f32; 2] = [0.0; 2],
        RelativeDirection: [f32; 2] = [0.0, -1.0],
        DeathSize: [f32; 2] = [0.0; 2],
        DeathDirection: [f32; 2] = [0.0; 2],
        DeathLocalStartPos: [f32; 2] = [0.0; 2],
        HelicopterVolumeScaleRange: [f32; 2] = [1.5, 5.5],
        Activated: bool = false,
        Mode: i32 = 0,
    }
    links {
        /// The component's instance id.
        this_id: i32 = 0,
        /// Behaviour.enabled.
        enabled: bool = false,
        /// The PhysicalBehaviour on its GameObject (its Transform's pose).
        this_physical: Option<usize> = None,
        /// Its GameObject or a descendant has a Collider2D or Rigidbody2D.
        this_physics: bool = false,
        HelicopterAudioSource: bool = false,
        FirstBladeRenderer: bool = false,
        SecondBladeRenderer: bool = false,
        Blades: Option<usize> = None,
        physicalBehaviour: Option<usize> = None,
        Audio: bool = false,
        Switch: bool = false,
        StartupSound: bool = false,
        ShutoffSound: bool = false,
        Mask: u32 = 0,
    }
}

impl Game {
    /// FanBehaviour.IsFan
    pub fn fan_behaviour_is_fan(&mut self, this: usize) -> bool {
        return (self.tr.fan_behaviour[this].Mode == 0);
    }

    /// FanBehaviour.Awake
    pub fn fan_behaviour_awake(&mut self, this: usize) {
        self.tr.fan_behaviour[this].physicalBehaviour = self.tr.fan_behaviour[this].this_physical;
        return;
    }

    /// FanBehaviour.Start
    pub fn fan_behaviour_start(&mut self, this: usize) {
        self.fan_behaviour_update_activated(this);
        return;
    }

    /// FanBehaviour.Use
    pub fn fan_behaviour_use(&mut self, this: usize, mut activation: rt::Activation) {
        if self.tr.fan_behaviour[this].enabled {
            self.tr.fan_behaviour[this].Activated = !self.tr.fan_behaviour[this].Activated;
            { let _ = self.tr.fan_behaviour[this].Switch; rt::pres(self.tr.fan_behaviour[this].Audio); };
            self.fan_behaviour_update_activated(this);
        }
        return;
    }

    /// FanBehaviour.Update
    pub fn fan_behaviour_update(&mut self, this: usize) {
        let mut value: f32 = 0.0;
        if self.tr.fan_behaviour[this].HelicopterAudioSource {
            value = { let a1 = self.tr_local_scale(rt::nn(Some(rt::Xf::this(self.tr.fan_behaviour[this].this_physical, self.tr.fan_behaviour[this].this_physics))))[0]; let a2 = self.tr_local_scale(rt::nn(Some(rt::Xf::this(self.tr.fan_behaviour[this].this_physical, self.tr.fan_behaviour[this].this_physics))))[1]; mathf::max(a1, a2) };
            { let _ = mathf::map_range(self.tr.fan_behaviour[this].HelicopterVolumeScaleRange[0], self.tr.fan_behaviour[this].HelicopterVolumeScaleRange[1], 0.0, 1.0, value); rt::pres(self.tr.fan_behaviour[this].HelicopterAudioSource); };
            { let _ = mathf::map_range(self.tr.fan_behaviour[this].HelicopterVolumeScaleRange[0], self.tr.fan_behaviour[this].HelicopterVolumeScaleRange[1], 1.0, 0.20000000298023224, value); rt::pres(self.tr.fan_behaviour[this].Audio); };
        }
        self.tr.cosmetic_rotation_behaviour[rt::nn(self.tr.fan_behaviour[this].Blades)].ShouldRotate = self.tr.fan_behaviour[this].Activated;
        self.tr.cosmetic_rotation_behaviour[rt::nn(self.tr.fan_behaviour[this].Blades)].RotationSpeedTarget = ((((9000.0 as f64) * ((self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].Charge() as f64) + (1.0 as f64)))) as f32);
        return;
    }

    /// FanBehaviour.FixedUpdate
    pub fn fan_behaviour_fixed_update(&mut self, this: usize) {
        let mut worldDir: [f32; 3] = [0.0; 3];
        if self.tr.fan_behaviour[this].Activated {
            worldDir = { let a1 = rt::v3(self.tr.fan_behaviour[this].RelativeDirection); self.tr_transform_vector(rt::nn(Some(rt::Xf::this(self.tr.fan_behaviour[this].this_physical, self.tr.fan_behaviour[this].this_physics))), a1) };
            let mut s1 = 0usize;
            'c1: loop {
                match s1 {
                    0 => {
                        match self.tr.fan_behaviour[this].Mode {
                            i32::MIN..=-1 | 2..=i32::MAX => {
                                break 'c1;
                            }
                            0 => {
                                s1 = 2; continue 'c1;
                            }
                            1 => {
                                s1 = 1; continue 'c1;
                            }
                        }
                    }
                    1 => {
                        self.fan_behaviour_fly(this, worldDir);
                        break 'c1;
                    }
                    2 => {
                        self.fan_behaviour_hover_and_push(this, worldDir);
                        self.fan_behaviour_kill(this);
                        break 'c1;
                    }
                    _ => unreachable!(),
                }
            }
        }
        return;
    }

    /// FanBehaviour.Fly
    pub fn fan_behaviour_fly(&mut self, this: usize, mut worldDir: [f32; 3]) {
        { let a1 = rt::v2(rt::mul(worldDir, ((((((self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].Charge() as f64) + (1.0 as f64)) * (50.0 as f64)) * (self.tr.fan_behaviour[this].Strength as f64))) as f32))); self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].rigidbody.add_force(&mut self.world, a1) };
        return;
    }

    /// FanBehaviour.HoverAndPush
    pub fn fan_behaviour_hover_and_push(&mut self, this: usize, mut worldDir: [f32; 3]) {
        let mut length: f32 = 0.0;
        length = (((((self.tr.fan_behaviour[this].Range as f64) * (2.0 as f64)) + ((self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].Charge() as f64) * (0.5 as f64)))) as f32);
        self.fan_behaviour_push(this, worldDir, length);
        self.fan_behaviour_hover(this, worldDir, length);
        return;
    }

    /// FanBehaviour.Kill
    pub fn fan_behaviour_kill(&mut self, this: usize) {
        let mut num: i32 = 0;
        let mut vector: [f32; 3] = [0.0; 3];
        let mut i: i32 = 0;
        let mut collider2D: i32 = 0;
        let mut value: Option<usize> = None;
        num = { let a2 = rt::v2({ let a1 = rt::v3(self.tr.fan_behaviour[this].DeathLocalStartPos); self.tr_transform_point(rt::nn(Some(rt::Xf::this(self.tr.fan_behaviour[this].this_physical, self.tr.fan_behaviour[this].this_physics))), a1) }); let a3 = [self.tr.fan_behaviour[this].DeathSize[0], self.tr.fan_behaviour[this].DeathSize[1]]; let a4 = self.tr_euler_angles(rt::nn(Some(rt::Xf::this(self.tr.fan_behaviour[this].this_physical, self.tr.fan_behaviour[this].this_physics))))[2]; let a5 = self.tr.fan_behaviour[this].Mask; { let h = self.tr_overlap_box(a2, a3, a4, a5); rt::fill(&mut self.tr.fan_behaviour__buffer, h) } };
        if (num == 0) {
            return;
        }
        vector = rt::mul({ let a6 = rt::v3(self.tr.fan_behaviour[this].DeathDirection); self.tr_transform_vector(rt::nn(Some(rt::Xf::this(self.tr.fan_behaviour[this].this_physical, self.tr.fan_behaviour[this].this_physics))), a6) }, (((((({ let a7 = ((((self.time.time as f64) * (self.cosmetic_rotation_behaviour_get_current_speed(rt::nn(self.tr.fan_behaviour[this].Blades)) as f64))) as f32); mathf::sin(a7) } as f64) * (self.cosmetic_rotation_behaviour_get_current_speed(rt::nn(self.tr.fan_behaviour[this].Blades)) as f64)) * (2.0 as f64)) * ((mathf::min(self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].Charge(), 2.0) as f64) + (1.0 as f64)))) as f32));
        i = 0;
        let mut s1 = 0usize;
        'c1: loop {
            match s1 {
                0 => {
                    if (i < num) {
                        s1 = 1; continue 'c1;
                    } else {
                        break 'c1;
                    }
                }
                1 => {
                    collider2D = self.tr.fan_behaviour__buffer[rt::index(i)];
                    if (!self.tr_xf_eq(Some(rt::Xf::Collider(collider2D)), Some(rt::Xf::this(self.tr.fan_behaviour[this].this_physical, self.tr.fan_behaviour[this].this_physics))) && { let r = self.tr_physical_by_transform(rt::nn(Some(rt::Xf::Collider(collider2D)))); value = r; r.is_some() }) {
                        self.physicals[rt::nn(value)].rigidbody.add_force(&mut self.world, rt::v2(vector));
                        self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].rigidbody.add_force(&mut self.world, rt::v2(rt::neg(vector)));
                    }
                    s1 = 2; continue 'c1;
                }
                2 => {
                    i = i.wrapping_add(1);
                    s1 = 0; continue 'c1;
                }
                _ => unreachable!(),
            }
        }
        return;
    }

    /// FanBehaviour.UpdateActivated
    pub fn fan_behaviour_update_activated(&mut self, this: usize) {
        if self.tr.fan_behaviour[this].Activated {
            { rt::pres(self.tr.fan_behaviour[this].Audio); };
            { let _ = self.tr.fan_behaviour[this].StartupSound; rt::pres(self.tr.fan_behaviour[this].Audio); };
            if self.tr.fan_behaviour[this].HelicopterAudioSource {
                { rt::pres(self.tr.fan_behaviour[this].HelicopterAudioSource); };
                { let _ = self.tr.fan_behaviour[this].StartupSound; rt::pres(self.tr.fan_behaviour[this].HelicopterAudioSource); };
            }
            return;
        }
        { rt::pres(self.tr.fan_behaviour[this].Audio); };
        if self.tr.fan_behaviour[this].HelicopterAudioSource {
            { rt::pres(self.tr.fan_behaviour[this].HelicopterAudioSource); };
        }
        if ((self.cosmetic_rotation_behaviour_get_current_speed(rt::nn(self.tr.fan_behaviour[this].Blades)) as f64) > (30.0 as f64)) {
            if self.tr.fan_behaviour[this].HelicopterAudioSource {
                { let _ = self.tr.fan_behaviour[this].ShutoffSound; rt::pres(self.tr.fan_behaviour[this].HelicopterAudioSource); };
            }
            { let _ = self.tr.fan_behaviour[this].ShutoffSound; rt::pres(self.tr.fan_behaviour[this].Audio); };
        }
        return;
    }

    /// FanBehaviour.Hover
    pub fn fan_behaviour_hover(&mut self, this: usize, mut worldDir: [f32; 3], mut length: f32) {
        let mut vector: [f32; 3] = [0.0; 3];
        let mut raycastHit2D: Option<RayHit2D> = None;
        let mut vector2: [f32; 2] = [0.0; 2];
        let mut vector3: [f32; 2] = [0.0; 2];
        if ((self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].Wetness as f64) > (0.20000000298023224 as f64)) {
            { let a1 = rt::v2(rt::mul(worldDir, (((((self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].Charge() as f64) + (1.0 as f64)) * (50.0 as f64))) as f32))); self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].rigidbody.add_force(&mut self.world, a1) };
        }
        vector = { let a2 = rt::v3(self.tr.fan_behaviour[this].LocalStartPos); self.tr_transform_point(rt::nn(Some(rt::Xf::this(self.tr.fan_behaviour[this].this_physical, self.tr.fan_behaviour[this].this_physics))), a2) };
        raycastHit2D = { let a3 = self.tr.fan_behaviour[this].Mask; self.tr_raycast(rt::v2(vector), rt::v2(rt::neg(worldDir)), length, a3) };
        if !raycastHit2D.is_some() {
            { let a4 = rt::v2(rt::mul(worldDir, (((((self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].Charge() as f64) + (1.0 as f64)) * (20.0 as f64))) as f32))); self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].rigidbody.add_force(&mut self.world, a4) };
            return;
        }
        { let _ = rt::v3(rt::hit_point(&raycastHit2D)); };
        vector2 = rt::v2(rt::sub(vector, rt::div(rt::mul(worldDir, length), 2.0)));
        { let _ = rt::v3(vector2); };
        vector3 = rt::mul(rt::sub(vector2, rt::hit_point(&raycastHit2D)), ((((-20.0 as f64) * (self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].rigidbody.mass(&self.world) as f64))) as f32));
        if !((rt::dot2(rt::v2(rt::normalized3(worldDir)), mathf::normalized2(vector3)) as f64) < (0.0 as f64)) {
            { let _ = self.tr_position(rt::nn(Some(rt::Xf::this(self.tr.fan_behaviour[this].this_physical, self.tr.fan_behaviour[this].this_physics)))); let _ = rt::v3(rt::add(rt::v2(self.tr_position(rt::nn(Some(rt::Xf::this(self.tr.fan_behaviour[this].this_physical, self.tr.fan_behaviour[this].this_physics))))), vector3)); };
            self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].rigidbody.add_force(&mut self.world, vector3);
            { let v = rt::mul(self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].rigidbody.velocity(&self.world), 0.9800000190734863); self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].rigidbody.set_velocity(&mut self.world, v); };
        }
        return;
    }

    /// FanBehaviour.Push
    pub fn fan_behaviour_push(&mut self, this: usize, mut worldDir: [f32; 3], mut length: f32) {
        let mut num: f32 = 0.0;
        let mut num2: i32 = 0;
        let mut vector: [f32; 3] = [0.0; 3];
        let mut i: i32 = 0;
        let mut collider2D: i32 = 0;
        let mut value: Option<usize> = None;
        num = ((((self.tr.fan_behaviour[this].Strength as f64) * ((1.0 as f64) + (self.physicals[rt::nn(self.tr.fan_behaviour[this].physicalBehaviour)].Charge() as f64)))) as f32);
        num2 = { let a2 = rt::v2({ let a1 = rt::v3(self.tr.fan_behaviour[this].LocalStartPos); self.tr_transform_point(rt::nn(Some(rt::Xf::this(self.tr.fan_behaviour[this].this_physical, self.tr.fan_behaviour[this].this_physics))), a1) }); let a3 = [self.tr.fan_behaviour[this].AreaWidth, length]; let a4 = self.tr.fan_behaviour[this].Mask; { let h = self.tr_overlap_box(a2, a3, (((((rt::atan2(worldDir[1], worldDir[0]) as f64) * (57.295780181884766 as f64)) - (90.0 as f64))) as f32), a4); rt::fill(&mut self.tr.fan_behaviour__buffer, h) } };
        vector = rt::mul(worldDir, ((((2.0 as f64) * ((0.0 as f64) - (num as f64)))) as f32));
        i = 0;
        let mut s1 = 0usize;
        'c1: loop {
            match s1 {
                0 => {
                    if (i < num2) {
                        s1 = 1; continue 'c1;
                    } else {
                        break 'c1;
                    }
                }
                1 => {
                    collider2D = self.tr.fan_behaviour__buffer[rt::index(i)];
                    if (!self.tr_xf_eq(Some(rt::Xf::Collider(collider2D)), Some(rt::Xf::this(self.tr.fan_behaviour[this].this_physical, self.tr.fan_behaviour[this].this_physics))) && { let r = self.tr_physical_by_transform(rt::nn(Some(rt::Xf::Collider(collider2D)))); value = r; r.is_some() }) {
                        { let a5 = rt::v2(rt::mul(vector, mathf::max(self.physicals[rt::nn(value)].rigidbody.mass(&self.world), 1.0))); self.physicals[rt::nn(value)].rigidbody.add_force(&mut self.world, a5) };
                        self.physicals[rt::nn(value)].Temperature = mathf::lerp(self.physicals[rt::nn(value)].Temperature, self.AmbientTemperature, 0.004999999888241291);
                    }
                    s1 = 2; continue 'c1;
                }
                2 => {
                    i = i.wrapping_add(1);
                    s1 = 0; continue 'c1;
                }
                _ => unreachable!(),
            }
        }
        return;
    }

    /// FanBehaviour.OnDisable
    pub fn fan_behaviour_on_disable(&mut self, this: usize) {
        self.tr.fan_behaviour[this].Activated = false;
        self.fan_behaviour_update_activated(this);
        return;
    }
}
