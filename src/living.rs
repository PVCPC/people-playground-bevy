use super::*;
use b2::world::CONTACT_TOUCHING;
use std::collections::HashSet;

#[derive(Component)]
pub struct Human {
    pub awake: bool,
    pub pose: usize,
    pub root: Entity,
    pub brain: Entity,
    pub grounded: bool,
    pub connected: HashSet<Entity>,
}

impl Human {
    fn capable(&self, entity: Entity) -> bool {
        self.awake && self.connected.contains(&self.brain) && self.connected.contains(&entity)
    }
}

#[derive(Component)]
pub struct Limb {
    pub owner: Entity,
    pub definition: usize,
}

#[derive(Component)]
pub struct MuscleJoint {
    pub owner: Entity,
    pub definition: usize,
    pub previous_speed: f32,
}

pub fn sense_humans(
    mut humans: Query<&mut Human>,
    limbs: Query<(Entity, &Body), With<Limb>>,
    joints: Query<&Hinge, With<MuscleJoint>>,
    physics: Res<Physics>,
) {
    let world = &physics.world;
    let limb_bodies: HashSet<_> = limbs.iter().map(|(_, body)| body.0).collect();
    for mut human in &mut humans {
        human.connected.clear();
        let root = human.root;
        human.connected.insert(root);
        loop {
            let old_len = human.connected.len();
            for joint in joints.iter().filter(|j| world.joints[j.joint].is_some()) {
                if human.connected.contains(&joint.body_a) {
                    human.connected.insert(joint.body_b);
                }
                if human.connected.contains(&joint.body_b) {
                    human.connected.insert(joint.body_a);
                }
            }
            if human.connected.len() == old_len {
                break;
            }
        }
        // shortcut: contact membership approximates support; use upward normals as obstacle geometry expands.
        human.grounded = limbs.iter().any(|(entity, body)| {
            human.connected.contains(&entity)
                && world.body(body.0).contacts.iter().any(|&(other, contact)| {
                    !limb_bodies.contains(&other)
                        && world.contact(contact).flags & CONTACT_TOUCHING != 0
                })
        });
    }
}

fn angle_error(current: f32, target: f32) -> f32 {
    (target - current + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI
}

pub fn drive_muscles(
    rig: Res<HumanRig>,
    session: Res<Session>,
    humans: Query<&Human>,
    mut joints: Query<(&Hinge, &mut MuscleJoint)>,
    bodies: Query<(Entity, &Limb, &Body)>,
    mut physics: ResMut<Physics>,
) {
    if session.paused {
        return;
    }
    let world = &mut physics.world;
    for (hinge, mut muscle) in &mut joints {
        let Some(joint) = world.joints[hinge.joint].as_mut() else {
            continue;
        };
        joint.revolute.def.enable_motor = false;
        let (body_a, body_b) = (joint.body_a, joint.body_b);
        let reference_angle = joint.revolute.def.reference_angle;
        let human = humans.get(muscle.owner).unwrap();
        let Some(pose) = rig.poses.get(human.pose) else {
            continue;
        };
        let part = &rig.parts[muscle.definition];
        let Some(spec) = &part.muscle else {
            continue;
        };
        if !human.capable(hinge.body_a) || !human.capable(hinge.body_b) {
            continue;
        }
        let target = pose.angles.iter().find(|angle| angle.part == part.name);
        let (child, mass) = (world.body(body_a).sweep.a, world.body(body_a).mass);
        let parent = world.body(body_b).sweep.a;
        let output = motor::command(&motor::MotorInput {
            current_angle: hinge::joint_angle(child, parent, reference_angle).to_degrees(),
            reference_angle: reference_angle.to_degrees(),
            target_angle: target.map_or(0.0, |t| t.degrees()),
            previous_speed: muscle.previous_speed,
            rigidity: pose.rigidity,
            rigidity_modifier: target.map_or(0.0, |t| t.rigidity_modifier),
            global_rigidity: 1.0,
            angle_offset: 1.0,
            mass,
            initial_mass: part.mass,
            base_strength: spec.strength,
            force_multiplier: pose.force_multiplier,
            active: target.is_some(),
            strength_multipliers: vec![],
        })
        .clamp_native_properties();
        muscle.previous_speed = output.speed;
        let def = &mut world.joints[hinge.joint].as_mut().unwrap().revolute.def;
        def.enable_motor = output.torque > 0.0;
        def.motor_speed = output.speed.to_radians();
        def.max_motor_torque = output.torque;
    }
    for (entity, limb, body) in &bodies {
        let human = humans.get(limb.owner).unwrap();
        let Some(pose) = rig.poses.get(human.pose) else {
            continue;
        };
        let Some(spec) = &rig.parts[limb.definition].muscle else {
            continue;
        };
        if !human.capable(entity) || !human.grounded || !pose.stand_upright {
            continue;
        }
        let body = world.body_mut(body.0);
        let angle = angle_error(body.sweep.a, 0.0);
        if spec.upright_force > 0.001 && angle.cos() >= 0.0 {
            let torque = angle.to_degrees()
                * spec.upright_force
                * spec.strength
                * 0.9
                * pose.force_multiplier
                * 1.8
                * pose.upright_multiplier;
            body.apply_torque(torque);
        }
        body.angular_velocity *= 1.0 - 0.08 * pose.drag_influence;
        body.linear_velocity = body.linear_velocity * (1.0 - 0.06 * pose.drag_influence);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upright_error_wraps_across_the_half_turn() {
        assert!(
            (angle_error(179f32.to_radians(), -179f32.to_radians()).to_degrees() - 2.0).abs()
                < 0.001
        );
    }
}
