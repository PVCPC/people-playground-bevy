//! Bevy side of the b2 port: one b2::World stepped after FixedUpdate (Unity steps physics after the
//! scripts' FixedUpdate). Entities carry body and joint ids; despawning one destroys it in the world.
use crate::b2::{
    math::Vec2 as V,
    shape::{box_corners, Polygon, POLYGON_RADIUS},
    world::{BodyId, Filter, FixtureDef, JointId, Shape, World},
};
use crate::game_assets::{BoxShape, SimulationSettings};
use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};

#[derive(Resource, Default)]
pub struct Physics {
    pub world: World,
    pub paused: bool,
    pub elapsed: f32,
}

#[derive(Component, Clone, Copy)]
#[component(on_remove = remove_body)]
pub struct Body(pub BodyId);

/// A HingeJoint2D: `body_a` is the attached body (FetchBodyA), `body_b` the connected one.
#[derive(Component, Clone, Copy)]
#[component(on_remove = remove_hinge)]
pub struct Hinge {
    pub joint: JointId,
    pub body_a: Entity,
    pub body_b: Entity,
}

fn remove_body(mut world: DeferredWorld, context: HookContext) {
    let id = world.get::<Body>(context.entity).unwrap().0;
    if let Some(mut physics) = world.get_resource_mut::<Physics>() {
        physics.world.destroy_body(id);
    }
}

fn remove_hinge(mut world: DeferredWorld, context: HookContext) {
    let id = world.get::<Hinge>(context.entity).unwrap().joint;
    if let Some(mut physics) = world.get_resource_mut::<Physics>()
        && physics.world.joints[id].is_some()
    {
        physics.world.destroy_joint(id);
    }
}

pub fn v(p: Vec2) -> V {
    V::new(p.x, p.y)
}

pub fn bevy(p: V) -> Vec2 {
    Vec2::new(p.x, p.y)
}

pub fn box_fixture(shape: &BoxShape, friction: f32, restitution: f32, filter: Filter) -> FixtureDef {
    let corners = box_corners(V::new(shape.offset[0], shape.offset[1]), V::new(shape.size[0], shape.size[1]));
    let shape = Shape::Polygon(Polygon::from_points(&corners, POLYGON_RADIUS));
    FixtureDef { shape, friction, restitution, density: 1.0, is_sensor: false, filter }
}

impl Physics {
    pub fn new(gravity: [f32; 2]) -> Self {
        Self { world: World::new(V::new(gravity[0], gravity[1])), ..Self::default() }
    }

    /// Rigidbody2D.position and .rotation (radians, unwrapped like b2Body::GetAngle).
    pub fn pose(&self, body: &Body) -> (Vec2, f32) {
        let b = self.world.body(body.0);
        (bevy(b.xf.p), b.sweep.a)
    }

    /// The world points both ends of a hinge pin to.
    pub fn anchors(&self, hinge: &Hinge) -> Option<(Vec2, Vec2)> {
        let j = self.world.joints[hinge.joint].as_ref()?;
        let a = self.world.body(j.body_a).xf.apply(j.revolute.def.local_anchor_a);
        let b = self.world.body(j.body_b).xf.apply(j.revolute.def.local_anchor_b);
        Some((bevy(a), bevy(b)))
    }
}

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimulationSettings>()
            .init_resource::<Physics>()
            .add_systems(FixedPostUpdate, step);
    }
}

fn step(
    mut physics: ResMut<Physics>,
    settings: Res<SimulationSettings>,
    time: Res<Time>,
    mut bodies: Query<(&Body, &mut Transform)>,
) {
    if !physics.paused {
        let dt = time.delta_secs();
        physics.world.step(dt, settings.velocity_iterations as i32, settings.position_iterations as i32);
        physics.elapsed += dt;
    }
    for (body, mut transform) in &mut bodies {
        let (position, angle) = physics.pose(body);
        transform.translation = position.extend(transform.translation.z);
        transform.rotation = Quat::from_rotation_z(angle);
    }
}
