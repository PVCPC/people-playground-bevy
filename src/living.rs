use super::*;
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
    limbs: Query<(Entity, &Limb, &CollidingEntities)>,
    joints: Query<&RevoluteJoint, With<MuscleJoint>>,
    supports: Query<(), (With<Collider>, Without<Limb>)>,
) {
    for mut human in &mut humans {
        human.connected.clear();
        let root = human.root;
        human.connected.insert(root);
        loop {
            let old_len = human.connected.len();
            for joint in &joints {
                if human.connected.contains(&joint.body1) {
                    human.connected.insert(joint.body2);
                }
                if human.connected.contains(&joint.body2) {
                    human.connected.insert(joint.body1);
                }
            }
            if human.connected.len() == old_len {
                break;
            }
        }
        // shortcut: contact membership approximates support; use upward normals as obstacle geometry expands.
        human.grounded = limbs.iter().any(|(entity, _, contacts)| {
            human.connected.contains(&entity)
                && contacts.iter().any(|other| supports.contains(*other))
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
    mut joints: Query<(&RevoluteJoint, &mut MuscleJoint, &mut motor::VelocityMotor)>,
    rotations_and_masses: Query<(&Rotation, &Mass)>,
    mut bodies: Query<(Entity, &Limb, &Rotation, Forces)>,
) {
    if session.paused {
        return;
    }
    for (joint, mut muscle, mut motor) in &mut joints {
        motor.enabled = false;
        let human = humans.get(muscle.owner).unwrap();
        let Some(pose) = rig.poses.get(human.pose) else {
            continue;
        };
        let part = &rig.parts[muscle.definition];
        let Some(spec) = &part.muscle else {
            continue;
        };
        if !human.capable(joint.body1) || !human.capable(joint.body2) {
            continue;
        }
        let target = pose.angles.iter().find(|angle| angle.part == part.name);
        let (child, mass) = rotations_and_masses.get(joint.body1).unwrap();
        let (parent, _) = rotations_and_masses.get(joint.body2).unwrap();
        let output = motor::command(&motor::MotorInput {
            current_angle: hinge::joint_angle(
                child.as_radians(),
                parent.as_radians(),
                motor.reference_angle,
            )
            .to_degrees(),
            reference_angle: motor.reference_angle.to_degrees(),
            target_angle: target.map_or(0.0, |t| t.degrees()),
            previous_speed: muscle.previous_speed,
            rigidity: pose.rigidity,
            rigidity_modifier: target.map_or(0.0, |t| t.rigidity_modifier),
            global_rigidity: 1.0,
            angle_offset: 1.0,
            mass: mass.0,
            initial_mass: part.mass,
            base_strength: spec.strength,
            force_multiplier: pose.force_multiplier,
            active: target.is_some(),
            strength_multipliers: vec![],
        })
        .clamp_native_properties();
        muscle.previous_speed = output.speed;
        motor.enabled = output.torque > 0.0;
        motor.target_velocity = output.speed.to_radians();
        motor.max_torque = output.torque;
    }
    for (entity, limb, rotation, mut forces) in &mut bodies {
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
        let angle = angle_error(rotation.as_radians(), 0.0);
        if spec.upright_force > 0.001 && angle.cos() >= 0.0 {
            let torque = angle.to_degrees()
                * spec.upright_force
                * spec.strength
                * 0.9
                * pose.force_multiplier
                * 1.8
                * pose.upright_multiplier;
            forces.apply_torque(torque);
        }
        *forces.angular_velocity_mut() *= 1.0 - 0.08 * pose.drag_influence;
        *forces.linear_velocity_mut() *= 1.0 - 0.06 * pose.drag_influence;
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
