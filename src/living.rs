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
    pub reference_angle: f32,
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

fn muscle_limits(rigidity: f32, modifier: f32, strength: f32, multiplier: f32) -> (f32, f32) {
    (
        // shortcut: Avian servo frequency needs runtime-reference calibration for exact motor parity.
        rigidity * (1.0 + modifier).max(0.0) * 2.0,
        strength * 0.9 * multiplier * (1.0 + modifier).clamp(0.0, 1.0),
    )
}

pub fn drive_muscles(
    rig: Res<HumanRig>,
    session: Res<Session>,
    humans: Query<&Human>,
    mut joints: Query<(&mut RevoluteJoint, &MuscleJoint)>,
    mut bodies: Query<(Entity, &Limb, &Rotation, Forces)>,
) {
    if session.paused {
        return;
    }
    for (mut joint, muscle) in &mut joints {
        let human = humans.get(muscle.owner).unwrap();
        let Some(pose) = rig.poses.get(human.pose) else {
            joint.motor.enabled = false;
            continue;
        };
        let part = &rig.parts[muscle.definition];
        let Some(spec) = &part.muscle else {
            joint.motor.enabled = false;
            continue;
        };
        if !human.capable(joint.body1) || !human.capable(joint.body2) {
            joint.motor.enabled = false;
            joint.motor.target_velocity = 0.0;
            continue;
        }
        let target = pose.angles.iter().find(|angle| angle.part == part.name);
        let (frequency, torque) = if let Some(target) = target {
            muscle_limits(
                pose.rigidity,
                target.rigidity_modifier,
                spec.strength,
                pose.force_multiplier,
            )
        } else {
            (0.0, spec.strength * 0.9)
        };
        let motor_model = if target.is_some() {
            // Use Avian's implicit servo to avoid injecting energy through linked limbs.
            MotorModel::SpringDamper {
                frequency,
                damping_ratio: 1.0,
            }
        } else {
            MotorModel::AccelerationBased {
                stiffness: 0.0,
                damping: 1.0,
            }
        };
        joint.motor = AngularMotor {
            enabled: torque > 0.0,
            target_position: target.map_or(0.0, |target| muscle.reference_angle - target.angle),
            target_velocity: 0.0,
            max_torque: torque,
            motor_model,
            ..default()
        };
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
    fn muscle_limits_reduce_torque_and_response_for_soft_limbs() {
        assert!(
            (angle_error(179f32.to_radians(), -179f32.to_radians()).to_degrees() - 2.0).abs()
                < 0.001
        );
        let (frequency, torque) = muscle_limits(4.0, 0.0, 10.0, 1.0);
        assert!((torque - 9.0).abs() < 0.001);
        let (soft_frequency, soft) = muscle_limits(4.0, -0.9, 10.0, 1.0);
        assert!((soft - 0.9).abs() < 0.001);
        assert!((soft_frequency - frequency * 0.1).abs() < 0.001);
    }
}
