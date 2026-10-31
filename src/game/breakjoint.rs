//! BreakJointOnBurnBehaviour and BreakJointOnShotBehaviour (the bushes' and trees' branches): burning lowers the
//! joint's breakForce every Update, a shot or slice can drop it to zero, and the script goes once its joint broke.
#![allow(non_snake_case)]

use super::Game;

#[derive(Clone, Debug, Default)]
pub struct BreakJointOnBurn {
    /// The component's instance id.
    pub id: i32,
    /// Joint: the Joint2D component's instance id.
    pub Joint: i32,
    /// phys: GetComponent<PhysicalBehaviour>() in Awake.
    pub phys: Option<usize>,
    pub initialThreshold: f32,
    pub randomOffset: f32,
}

#[derive(Clone, Debug, Default)]
pub struct BreakJointOnShot {
    pub id: i32,
    /// Joint (OnEnable's GetComponent<Joint2D>() when the prefab leaves it null; None: still null).
    pub Joint: Option<i32>,
    pub Chance: f32,
    pub ShotIntensityInfluence: f32,
}

impl Game {
    /// BreakJointOnBurnBehaviour.Awake: phys and initialThreshold are linked; Random.Range(1, 3) is drawn here.
    pub fn break_joint_on_burn_awake(&mut self, b: usize) {
        self.break_joint_on_burns[b].randomOffset = self.random.range_int(1, 3) as f32;
    }

    /// BreakJointOnBurnBehaviour.Update
    pub fn break_joint_on_burn_update(&mut self, b: usize) {
        let s = &self.break_joint_on_burns[b];
        if self.destroyed_components.contains(&s.Joint) {
            return;
        }
        let p = s.phys.unwrap_or_else(|| unimplemented!("BreakJointOnBurnBehaviour.Update without a PhysicalBehaviour (NullReferenceException)"));
        let num = (1.0 - self.physicals[p].BurnProgress * s.randomOffset).clamp(0.0, 1.0);
        let (joint, v) = (s.Joint, s.initialThreshold * num);
        self.joint_set_break_force(joint, v);
    }

    /// OnJointBreak2D of both classes on the broken joint's GameObject: `joint == Joint` queues Object.Destroy(this).
    pub fn break_joint_scripts_on_break(&mut self, joint: i32) {
        let ids: Vec<i32> = self.break_joint_on_burns.iter().filter(|s| s.Joint == joint).map(|s| s.id)
            .chain(self.break_joint_on_shots.iter().filter(|s| s.Joint == Some(joint)).map(|s| s.id)).collect();
        for id in ids {
            self.tr_destroy_component(id, 0.0);
        }
    }

    /// BreakJointOnShotBehaviour.Shot (component `id`): the draw happens whether or not the joint is still there.
    pub fn break_joint_on_shot_shot(&mut self, id: i32, damage: f32) {
        if self.destroyed_components.contains(&id) {
            return;
        }
        let s = self.break_joint_on_shots.iter().find(|s| s.id == id).expect("a BreakJointOnShotBehaviour");
        let (num, joint) = (s.Chance + damage * s.ShotIntensityInfluence, s.Joint);
        if self.random.value() < num && let Some(j) = joint.filter(|j| !self.destroyed_components.contains(j)) {
            self.joint_set_break_force(j, -1.0);
        }
    }

    /// BreakJointOnShotBehaviour.Slice: `Joint.breakForce = -1f` (a destroyed or null joint throws).
    pub fn break_joint_on_shot_slice(&mut self, id: i32) {
        if self.destroyed_components.contains(&id) {
            return;
        }
        let s = self.break_joint_on_shots.iter().find(|s| s.id == id).expect("a BreakJointOnShotBehaviour");
        let j = s.Joint.filter(|j| !self.destroyed_components.contains(j));
        let j = j.unwrap_or_else(|| unimplemented!("BreakJointOnShotBehaviour.Slice of a destroyed joint (NullReferenceException)"));
        self.joint_set_break_force(j, -1.0);
    }
}
