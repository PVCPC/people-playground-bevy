//! ToolControllerBehaviour with its SelectionController and DragTool (the tool the game starts with), fed by
//! InputSystem's controls and Global.MousePosition.
use super::activator_electrode::Act;
use super::*;
use crate::b2::math::Vec2 as V;
use crate::b2::revolute::RevoluteDef;
use crate::b2::world::{BodyId, JointId, Shape};

/// InputSystem.Down / Held / Up of one control this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Control {
    pub down: bool,
    pub held: bool,
    pub up: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Input {
    /// camera.ScreenToWorldPoint(Input.mousePosition) at the camera's distance: what Global.UpdateMousePosition
    /// stores this frame.
    pub mouse: [f32; 3],
    /// The bound controls by InputSystem name; a missing one reads false.
    pub controls: HashMap<String, Control>,
    /// Input.mousePosition and the main camera, in a window (recordings carry only the world position).
    pub screen: Option<Screen>,
    /// The main camera's orthographic view (world min, max), in a window.
    pub view: Option<([f32; 2], [f32; 2])>,
}

/// Input.mousePosition (pixels from the bottom left) and the orthographic camera's ScreenToWorldPoint, world =
/// origin + pixel × scale.
#[derive(Clone, Copy, Debug, Default)]
pub struct Screen {
    pub mouse: [f32; 2],
    pub origin: [f32; 2],
    pub scale: f32,
}

impl Screen {
    pub fn to_world(&self, p: [f32; 2]) -> [f32; 2] {
        [self.origin[0] + p[0] * self.scale, self.origin[1] + p[1] * self.scale]
    }
}

impl Input {
    pub fn down(&self, name: &str) -> bool {
        self.controls.get(name).is_some_and(|c| c.down)
    }
    pub fn held(&self, name: &str) -> bool {
        self.controls.get(name).is_some_and(|c| c.held)
    }
    pub fn up(&self, name: &str) -> bool {
        self.controls.get(name).is_some_and(|c| c.up)
    }
}

/// The HingeJoint2D DragTool adds to the ToolController: its JointMotor2D and useMotor, and the b2 joint.
#[derive(Clone, Debug)]
pub struct DragJoint {
    pub joint: Option<JointId>,
    /// JointMotor2D.motorSpeed (degrees per second) and maxMotorTorque.
    pub motorSpeed: f32,
    pub maxMotorTorque: f32,
    pub useMotor: bool,
    /// When Object.Destroy's DelayedCallManager entry runs.
    pub destroy_at: Option<f64>,
}

/// The ToolController object: ToolControllerBehaviour, its SelectionController, the DragTool and their Rigidbody2D.
#[derive(Clone, Debug)]
pub struct Tool {
    pub body: BodyId,
    /// The ToolController Transform (a root object).
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    // ToolControllerBehaviour
    pub SelectionLayer: u32,
    pub isCurrentlyHolding: bool,
    pub currentlyHovering: Option<usize>,
    pub isSelecting: bool,
    pub selectionDragStart: [f32; 2],
    // SelectionController (physicals)
    pub rawSelection: Vec<usize>,
    pub hover: Option<usize>,
    // DragTool
    pub offset: [f32; 2],
    pub dragJoint: Option<DragJoint>,
    /// Destroyed drag joints whose delayed destroy has not run yet.
    pub destroying: Vec<DragJoint>,
    pub controlRotation: bool,
    pub selectedRigidbody: Option<usize>,
    pub rotationSpeed: f32,
    pub currentRotation: f32,
    pub snappedMouseDelta: [f32; 3],
    pub prevMousePos: [f32; 3],
    pub initialSnappingFinished: bool,
    pub isActive: bool,
    pub shouldSendContinuousUseMessage: bool,
    pub initialTranslationSnapOffset: [f32; 3],
    pub ActiveSingleSelected: Option<usize>,
}

/// UserPreferences.ToolSelectionMode
pub const SELECTION_AREA: i32 = 0;
pub const SELECTION_MASS: i32 = 1;

const DEG2RAD: f32 = f32::from_bits(0x3c8efa35);

/// A receiver of BroadcastMessage("Use"): a rewritten behaviour or a UseEventTrigger (its component id).
enum UseReceiver {
    Behaviour(Behaviour),
    Event(i32),
}

/// The classes with a Use(ActivationPropagation) method, and their subclasses.
const USE_RECEIVERS: &[&str] = &[
    "AcidSyringe", "ActivatorElectrodeBehaviour", "AdhesiveCouplerBehaviour", "AdrenalineSyringe",
    "AerialFaithPlateBehaviour", "AndroidLaserBehaviour", "ArchelixCasterBehaviour", "AtomBombBehaviour",
    "BatteryAttachmentBehaviour", "BeamCannonBehaviour", "BeamRepeaterBehaviour", "BeamformerBehaviour",
    "BellBehaviour", "BlasterBehaviour", "BloodTankBehaviour", "BoatMotorBehaviour", "BoneEatingPoisonSyringe",
    "BulbBehaviour", "CarBehaviour", "CardiopulmonaryBypassMachineBehaviour", "CathodeLightBehaviour",
    "CentrifugeBehaviour", "ChainsawBehaviour", "CoagulationSyringe", "ConveyorBeltBehaviour", "CounterBehaviour",
    "DeathSyringe", "DecimatorBehaviour", "DefibrillatorBehaviour", "DetectorBehaviour", "DisplayBehaviour",
    "DroneBehaviour", "EBikeBehaviour", "EMPBehaviour", "EffectGunBehaviour", "ElectricityTransformerBehaviour",
    "EmptySyringe", "EnergySwordBehaviour", "ExcaliburBehaviour", "ExplosiveAttachmentBehaviour",
    "ExplosiveBehaviour", "FanBehaviour", "FirearmAttachmentBehaviour", "FirearmBehaviour", "FireworksBehaviour",
    "FlareBehaviour", "FlashlightAttachmentBehaviour", "ForcefieldGeneratorBehaviour", "FreezeGunBehaviour",
    "FreezeSyringe", "GeneratorBehaviour", "GenericScifiWeapon40Behaviour", "GlowtubeBehaviour", "GripBehaviour",
    "GyrostabiliserBehaviour", "HealthGunBehaviour", "HeartMonitorBehaviour", "HeatingElementBehaviour",
    "HoverThrusterBehaviour", "HoverboxBehaviour", "HovercarBehaviour", "ImmobilityFieldBehaviour",
    "IncendiaryAttachmentBehaviour", "IndustrialGyrostabiliserBehaviour", "InfraredThermometer",
    "IonThrusterBehaviour", "JetEngineBehaviour", "JukeboxBehaviour", "KnockoutSyringe", "LEDBulbBehaviour",
    "LagboxBehaviour", "LaserBehaviour", "LaserTurretBehaviour", "LifeSyringe", "LiquidDuplicatorBehaviour",
    "LiquidPressureValveBehaviour", "LiquidPumpBehaviour", "LiquidentifierBehaviour", "MachineGunBehaviour",
    "MagnetBehaviour", "MendingSyringe", "MetronomeBehaviour", "MinigunBehaviour", "MotorisedHingeBehaviour",
    "ObstructionEmitterBehaviour", "PainKillerSyringe", "ParticleMachineBehaviour", "PhysicalBehaviour",
    "PinkSyringe", "PistonBehaviour", "PointerBehaviour", "ProjectileLauncherBehaviour", "PulseDrumBehaviour",
    "RailgunBehaviour", "RayCannonBehaviour", "RaygunBehaviour", "RocketLauncherBehaviour", "RotorBehaviour",
    "ScherbewerferBehaviour", "ScopeAttachmentBehaviour", "SeismicChargeBehaviour", "SignalConverterBehaviour",
    "SingleFloodlightBehaviour", "SirenBehaviour", "SpikeGrenadeBehaviour", "SyringeBehaviour", "TankBarrelBehaviour",
    "TelevisionBehaviour", "TeslaCoilBehaviour", "ThermometerBehaviour", "ThrusterbedBehaviour", "TimedGateBehaviour",
    "TransmitterBehaviour", "TruckDoorBehaviour", "UltraStrengthSyringe", "UniversalBellBehaviour", "UseBubbler",
    "UseEventTrigger", "UseWireBehaviour", "ValveBehaviour", "WaterBreathingSyringe", "WinchBehaviour",
    "ZombieSyringe",
];

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// b2PolygonShape::TestPoint (180bb6060) and b2CircleShape::TestPoint (180bb5f20).
pub(super) fn test_point(shape: &Shape, xf: &crate::b2::math::Transform, p: V) -> bool {
    let (s, c) = (xf.q.s, xf.q.c);
    match shape {
        Shape::Polygon(poly) => {
            let (dx, dy) = (p.x - xf.p.x, p.y - xf.p.y);
            let local = V::new(c * dx + s * dy, c * dy - s * dx);
            let polygon_radius = crate::b2::settings::globals().polygon_radius;
            let n = poly.count;
            let sep = |i: usize| (local.x - poly.vertices[i].x) * poly.normals[i].x + (local.y - poly.vertices[i].y) * poly.normals[i].y;
            if !(polygon_radius < poly.radius) {
                return (0..n).all(|i| !(sep(i) > 0.0));
            }
            // A rounded polygon: within its extra radius of the first edge the point is outside.
            let Some(i) = (0..n).find(|&i| sep(i) > 0.0) else { return true };
            let near = nearest_point_on_segment(local, poly.vertices[i], poly.vertices[if i + 1 == n { 0 } else { i + 1 }]);
            let r = poly.radius - polygon_radius;
            let (nx, ny) = (near.x - local.x, near.y - local.y);
            r * r >= ny * ny + nx * nx
        }
        Shape::Circle(circle) => {
            let cx = (circle.p.x * c - s * circle.p.y) + xf.p.x;
            let cy = (s * circle.p.x + circle.p.y * c) + xf.p.y;
            let (dx, dy) = (p.x - cx, p.y - cy);
            circle.radius * circle.radius >= dy * dy + dx * dx
        }
        // b2ChainShape::TestPoint (180bb5de0): within the radius less b2_polygonRadius of any segment.
        Shape::Capsule(capsule) => capsule.test_point(xf, p),
        Shape::Chain(chain) => {
            let (dx, dy) = (p.x - xf.p.x, p.y - xf.p.y);
            let local = V::new(c * dx + s * dy, c * dy - s * dx);
            let r = chain.radius - crate::b2::settings::globals().polygon_radius;
            chain.vertices.windows(2).any(|w| {
                let near = nearest_point_on_segment(local, w[0], w[1]);
                let (nx, ny) = (near.x - local.x, near.y - local.y);
                r * r >= nx * nx + ny * ny
            })
        }
    }
}

use crate::b2::shape::nearest_point_on_segment;

impl Game {
    /// Physics2D.OverlapPointNonAlloc(point, buffer[16], layerMask, minDepth, maxDepth) as the physicals hit:
    /// PhysicsQuery2D::OverlapPointAll (180bf4d70) runs b2World::QueryAABB on the point; ReportFixture (180bf72d0)
    /// skips triggers (queriesHitTriggers is off), other layers, Transforms outside the depth range, a collider
    /// already hit and shapes that don't contain the point; the hits go by ascending Transform z (stable).
    pub(super) fn overlap_point(&mut self, point: [f32; 3], mask: u32, min_depth: f32, max_depth: f32) -> Vec<usize> {
        self.sync_pending();
        let p = V::new(point[0], point[1]);
        let hits = self.overlap_query(self.world.query_point(p), mask, min_depth, max_depth, |shape, _, xf| test_point(shape, xf, p));
        if hits.len() > 16 {
            unimplemented!("OverlapPointNonAlloc: more hits than its buffer");
        }
        // Every caller takes the collider's PhysicalBehaviour (TryGetComponent, the victim's GameObject or
        // PhysicalObjectsInWorldByTransform): a bare collider only fills its buffer slot.
        hits.into_iter().filter_map(|h| h.0).collect()
    }

    /// Physics2D.OverlapPoint(point, mask): the lowest-z collider at the point.
    pub fn overlap_point_first(&mut self, point: [f32; 2], mask: u32) -> Option<(Option<usize>, i32)> {
        self.overlap_point_depth(point, mask, f32::NEG_INFINITY, f32::INFINITY)
    }

    /// Physics2D.OverlapPoint(point, mask, minDepth, maxDepth): the lowest-z collider at the point in the range.
    pub fn overlap_point_depth(&mut self, point: [f32; 2], mask: u32, min_depth: f32, max_depth: f32) -> Option<(Option<usize>, i32)> {
        self.sync_pending();
        let p = V::new(point[0], point[1]);
        self.overlap_query(self.world.query_point(p), mask, min_depth, max_depth, |shape, _, xf| test_point(shape, xf, p)).first().copied()
    }

    /// Physics2D.OverlapAreaAll(a, b, mask): OverlapAreaAllToBox_Internal's OverlapBoxAll at the midpoint, size
    /// |a − b|, angle 0, depths unbounded. OverlapBoxQuery2D: SetAsBox(max(size/2, 1e-4)) with b2_polygonRadius,
    /// its AABB at the identity, and TestShapeOverlap (b2TestOverlap) per fixture.
    fn overlap_area_all(&mut self, a: [f32; 2], b: [f32; 2], mask: u32) -> Vec<usize> {
        self.sync_pending();
        let point = V::new((a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5);
        let size = [(a[0] - b[0]).abs(), (a[1] - b[1]).abs()];
        let min = f32::from_bits(0x38d1b717);
        let shape = Shape::Polygon(crate::b2::shape::Polygon::set_as_box((size[0] * 0.5).max(min), (size[1] * 0.5).max(min), point, crate::b2::settings::globals().polygon_radius));
        let Shape::Polygon(poly) = &shape else { unreachable!() };
        let (mut lower, mut upper) = (poly.vertices[0], poly.vertices[0]);
        for v in &poly.vertices[1..4] {
            (lower, upper) = (V::new(lower.x.min(v.x), lower.y.min(v.y)), V::new(upper.x.max(v.x), upper.y.max(v.y)));
        }
        let r = poly.radius;
        let aabb = crate::b2::broadphase::Aabb { lower: V::new(lower.x - r, lower.y - r), upper: V::new(upper.x + r, upper.y + r) };
        let identity = crate::b2::math::Transform { p: V::ZERO, q: crate::b2::math::Rot::new(0.0) };
        // The selection keeps the colliders whose Transform is in PhysicalObjectsInWorldByTransform.
        self.overlap_query(self.world.query_aabb(&aabb), mask, f32::NEG_INFINITY, f32::INFINITY, |other, child, xf| shape.test_overlap(0, &identity, other, child, xf)).into_iter().filter_map(|h| h.0).collect()
    }

    /// Collider2D.OverlapCollider(ContactFilter2D, Collider2D[1024]) on the collider whose fixtures are `query`
    /// (OverlapColliderQuery2D, 180bf1240 and Overlap2DQueryBase::RunQuery): AutoSyncTransforms, then b2World::QueryAABB
    /// over the union of every child's ComputeAABB, ReportFixture's filters as `overlap_query` (the filter's depth
    /// and normal-angle tests are off), and IsFixtureValid: any child of the query shapes against the other shape's
    /// children. The hits are sorted by z with std::sort (`sort::msvc_sort`), after the query collider's own are dropped.
    pub(super) fn overlap_collider(&mut self, query: &[crate::b2::world::FixtureId], mask: u32) -> Vec<(Option<usize>, i32)> {
        self.sync_pending();
        let shapes: Vec<(&Shape, crate::b2::math::Transform)> = query.iter().map(|&f| self.world.fixture(f)).map(|fx| (&fx.shape, self.world.body(fx.body).xf)).collect();
        let mut aabb: Option<crate::b2::broadphase::Aabb> = None;
        for &(shape, xf) in &shapes {
            for c in 0..shape.child_count() {
                let b = shape.compute_aabb(&xf, c);
                aabb = Some(match aabb {
                    None => b,
                    Some(a) => crate::b2::broadphase::Aabb { lower: V::new(a.lower.x.min(b.lower.x), a.lower.y.min(b.lower.y)), upper: V::new(a.upper.x.max(b.upper.x), a.upper.y.max(b.upper.y)) },
                });
            }
        }
        let Some(aabb) = aabb else { return Vec::new() };
        let mut hits = self.overlap_query_unsorted(self.world.query_aabb(&aabb), mask, f32::NEG_INFINITY, f32::INFINITY, |other, _, xf| {
            shapes.iter().any(|&(shape, xa)| (0..shape.child_count()).any(|ca| (0..other.child_count()).any(|cb| shape.test_overlap(ca, &xa, other, cb, xf))))
        });
        // ReportFixture skips the query collider's own fixtures.
        let own: Vec<i32> = query.iter().map(|&f| self.world.fixture(f).collider.id).collect();
        hits.retain(|h| !own.contains(&h.1));
        super::sort::msvc_sort(&mut hits, |a, b| a.2 < b.2);
        hits.into_iter().map(|(ph, c, _)| (ph, c)).collect()
    }

    /// The overlap queries' shared filter: sensors skipped (QueriesHitTriggers off), the layer mask, the depth
    /// range on the Transform's z, each collider once, then std::sort by z (`sort::msvc_sort`). The callers run PhysicsQuery2D's
    /// AutoSyncTransforms (Physics2DSettings.autoSyncTransforms is on) before querying.
    /// The hits as (the collider's physical, or None for a GameObject without one; the Collider2D id).
    pub(super) fn overlap_query(&self, fixtures: Vec<(crate::b2::world::FixtureId, usize)>, mask: u32, min_depth: f32, max_depth: f32, test: impl Fn(&Shape, usize, &crate::b2::math::Transform) -> bool) -> Vec<(Option<usize>, i32)> {
        let mut hits = self.overlap_query_unsorted(fixtures, mask, min_depth, max_depth, test);
        super::sort::msvc_sort(&mut hits, |a, b| a.2 < b.2);
        hits.into_iter().map(|(ph, c, _)| (ph, c)).collect()
    }

    /// `overlap_query` before the sort: the hits with their z, in the broadphase's report order.
    fn overlap_query_unsorted(&self, fixtures: Vec<(crate::b2::world::FixtureId, usize)>, mask: u32, min_depth: f32, max_depth: f32, test: impl Fn(&Shape, usize, &crate::b2::math::Transform) -> bool) -> Vec<(Option<usize>, i32, f32)> {
        let mut hits: Vec<(Option<usize>, i32, f32)> = Vec::new();
        for (f, child) in fixtures {
            let fx = self.world.fixture(f);
            if fx.is_sensor {
                continue;
            }
            let Some(&layer) = self.world.layers.get(&fx.collider.game_object) else {
                panic!("OverlapPoint: no layer for collider {}", fx.collider.id);
            };
            if mask & (1 << layer) == 0 {
                continue;
            }
            let ph = self.physical_of_collider(fx.collider.id);
            let z = match ph {
                Some(ph) => self.physicals[ph].transform.position[2],
                None => self.bare_position(fx.collider.id)[2],
            };
            if z < min_depth || max_depth < z {
                continue;
            }
            if hits.iter().any(|&(_, c, _)| c == fx.collider.id) {
                continue;
            }
            if test(&fx.shape, child, &self.world.body(fx.body).xf) {
                hits.push((ph, fx.collider.id, z));
            }
        }
        hits
    }

    /// Transform.position of a bare collider's GameObject: its local position through its chain, then the anchor's.
    pub(super) fn bare_position(&self, collider: i32) -> [f32; 3] {
        let Some(b) = self.bare_colliders.get(&collider) else { panic!("collider {collider} is neither a physical's nor bare") };
        self.anchored_world(b.anchor, &b.chain).0
    }

    /// The Transforms of an object `chain` below `anchor`: the object first, then each ancestor up to the root.
    pub fn anchored_nodes(&self, anchor: super::BareAnchor, chain: &[crate::unity::transform::Trs]) -> Vec<crate::unity::transform::Trs> {
        let mut nodes = chain.to_vec();
        match anchor {
            super::BareAnchor::Physical(p) => nodes.extend(self.local_chain(p).into_iter().rev()),
            super::BareAnchor::Node(mut i) => loop {
                nodes.push(self.node_trs(i));
                let Some(up) = self.transform_nodes[i].parent else { break };
                i = up;
            },
            super::BareAnchor::Root => {}
        }
        nodes
    }

    /// Transform.InverseTransformVector (1807f29f0 -> 1807f27e0) of an object `chain` below `anchor`: each
    /// ancestor's inverse from the root down, then the object's.
    pub fn anchored_inverse_vector(&self, anchor: super::BareAnchor, chain: &[crate::unity::transform::Trs], v: [f32; 3]) -> [f32; 3] {
        let mut v = [v[0], v[1], v[2], 0.0];
        for node in self.anchored_nodes(anchor, chain).iter().rev() {
            v = crate::unity::transform::inverse_vector(node, v);
        }
        [v[0], v[1], v[2]]
    }

    /// Transform.position, rotation and the product of the local scales of an object `chain` (object first) below
    /// `anchor`.
    pub fn anchored_world(&self, anchor: super::BareAnchor, chain: &[crate::unity::transform::Trs]) -> ([f32; 3], [f32; 4], [f32; 3]) {
        use crate::unity::transform::{global_position, global_rotation};
        let nodes = self.anchored_nodes(anchor, chain);
        let (mut v, mut r, mut s) = ([0.0; 4], [0.0, 0.0, 0.0, 1.0], [1.0f32; 3]);
        for n in &nodes {
            (v, r) = (global_position(n, v), global_rotation(n, r));
            s = [s[0] * n.s[0], s[1] * n.s[1], s[2] * n.s[2]];
        }
        ([v[0], v[1], v[2]], r, s)
    }


    /// The physical's Transform ancestors, root first, then its own local TRS.
    pub(super) fn local_chain(&self, p: usize) -> Vec<crate::unity::transform::Trs> {
        use crate::unity::transform::{inverse, Trs};
        let mut chain = Vec::new();
        let mut n = self.physicals[p].parent;
        while let Some(i) = n {
            chain.push(self.node_trs(i));
            n = self.transform_nodes[i].parent;
        }
        chain.reverse();
        let t = &self.physicals[p].transform;
        let (pos, rot) = match t.local {
            Some((p, q)) => ([p[0], p[1], p[2], 0.0], q),
            None => {
                let (mut pos, mut rot) = ([t.position[0], t.position[1], t.position[2], 0.0], t.rotation);
                for node in &chain {
                    (pos, rot) = inverse(node, pos, rot);
                }
                (pos, crate::unity::transform::normalize(rot))
            }
        };
        let s = t.localScale;
        chain.push(Trs { t: pos, q: rot, s: [s[0], s[1], s[2], 0.0] });
        chain
    }

    /// Transform.InverseTransformPoint (1807f2600): InverseTransformPosition (180171750) from the root down.
    pub fn inverse_transform_point(&self, p: usize, point: [f32; 3]) -> [f32; 3] {
        let mut v = [point[0], point[1], point[2], 0.0];
        for node in &self.local_chain(p) {
            v = crate::unity::transform::inverse(node, v, [0.0, 0.0, 0.0, 1.0]).0;
        }
        [v[0], v[1], v[2]]
    }

    /// Transform.TransformPoint (1807f6830): CalculateGlobalPosition's step through the object, then each parent.
    pub fn transform_point3(&self, p: usize, point: [f32; 3]) -> [f32; 3] {
        let mut v = [point[0], point[1], point[2], 0.0];
        for node in self.local_chain(p).iter().rev() {
            v = crate::unity::transform::global_position(node, v);
        }
        [v[0], v[1], v[2]]
    }

    /// Transform.TransformDirection of a physical object.
    pub fn transform_direction(&self, p: usize, dir: [f32; 3]) -> [f32; 3] {
        let mut v = [dir[0], dir[1], dir[2], 0.0];
        for (i, node) in self.local_chain(p).iter().rev().enumerate() {
            v = crate::unity::transform::rotate_direction(node.q, (i > 0).then_some(node.s), v);
        }
        [v[0], v[1], v[2]]
    }

    /// Transform.lossyScale of a physical object.
    pub fn lossy_scale(&self, p: usize) -> [f32; 3] {
        let mut nodes = self.local_chain(p);
        nodes.reverse();
        crate::unity::transform::lossy_scale(&nodes, self.physicals[p].transform.rotation)
    }

    /// DragTool.SnapToGrid
    fn snap_to_grid(&self, v: [f32; 3]) -> [f32; 3] {
        // WorldGridOffset.Offset: the Default map has no WorldGridOffset, so it stays zero.
        let offset = [0.0f32; 3];
        let s = 1.2136364f32 * self.prefs.GridSnap;
        // Utils.Snap(Vector3, float): each component Mathf.Round'ed to the step.
        let snap = |x: f32| if mathf::approximately(s, 0.0) { x } else { (x / s).round_ties_even() * s };
        let w = [v[0] + offset[0], v[1] + offset[1], v[2] + offset[2]];
        sub3([snap(w[0]), snap(w[1]), snap(w[2])], offset)
    }

    fn mouse_pos_with_snap(&self) -> [f32; 3] {
        self.snap_to_grid(self.MousePosition)
    }

    /// ToolControllerBehaviour.FixedUpdate
    pub fn tool_fixed_update(&mut self) {
        // DistanceWireTool.OnFixedHold is empty.
        if self.tool.is_some() && !self.wire_tool.chosen && self.input.held("drag") {
            self.drag_on_fixed_hold();
        }
    }

    /// ToolControllerBehaviour.Update (no menu, dialog or UI block is ever open).
    pub fn tool_update(&mut self) {
        let Some(t) = self.tool.as_mut() else { return };
        t.currentlyHovering = None;
        // HandleToolSelectionInput: "tool1".."tool10".
        if (1..11).any(|i| self.input.down(&format!("tool{i}"))) {
            unimplemented!("ToolControllerBehaviour.SetTool: tools other than DragTool");
        }
        // HandleIndirectInteraction
        if self.input.down("activateDirect") && self.tool.as_ref().unwrap().ActiveSingleSelected.is_none() {
            if let Some(p) = self.tool.as_ref().unwrap().hover {
                self.activate_direct(p);
            }
        }
        self.determine_hovering();
        self.handle_tools();
        // HandleContextMenu (ContextMenuBehaviour.Show is UI); UpdateSelectionBox draws the box.
    }

    /// A direct activation of a physical object: Utils.Activations.SendOnce, then BroadcastMessage("Use").
    pub(super) fn activate_direct(&mut self, p: usize) {
        // SendOnce: OnSingleUse.Invoke: the ActivationToggleBehaviour listeners.
        self.activation_toggle_use_direct(p);
        self.broadcast_use(p, &Act::direct(0));
    }

    /// BroadcastMessage("Use") on a physical object's GameObject: it and its active descendants, each object's
    /// components in order.
    /// PhysicalBehaviour.ForceSendUse: BroadcastMessage("Use", ActivationPropagation(root, channel, root), DontRequireReceiver)
    /// for each channel. PhysicalBehaviour.Use only invokes the modding event, so an object with no other Use receiver
    /// below its root (including itself) takes no effect; a receiver would need the channel and the non-direct flag.
    pub(super) fn physical_force_send_use(&mut self, p: usize) {
        if self.physicals[p].parent.is_some() {
            unimplemented!("PhysicalBehaviour.ForceSendUse of an object with a parent (BroadcastMessage on transform.root)");
        }
        let id = self.physicals[p].id;
        let receiver = self.with_component_instance(id, |_, inst, start| {
            std::iter::once(start).chain(inst.descendants(start)).filter(|&x| inst.objects[x].active).find_map(|x| {
                inst.prefab.objects[x].components.iter().find_map(|(_, c)| match c {
                    crate::assets::db::Component::Script { class, .. } if USE_RECEIVERS.contains(&class.as_str()) && class != "PhysicalBehaviour" => Some(class.clone()),
                    _ => None,
                })
            })
        });
        if let Some(class) = receiver {
            unimplemented!("PhysicalBehaviour.ForceSendUse: {class}.Use (its channel and non-direct activation)");
        }
    }

    /// The Use receivers BroadcastMessage("Use") reaches from `start` of `inst`: each active object's components in order, its
    /// children, then the clones parented under it (the last of its children), such as an attachment re-parented below a firearm.
    fn use_receivers_below(&mut self, inst: &super::spawn::Instance, start: usize) -> Vec<UseReceiver> {
        let mut out = Vec::new();
        let mut open: Vec<usize> = Vec::new();
        for x in std::iter::once(start).chain(inst.descendants(start)) {
            while let Some(&top) = open.last() {
                if inst.descendants(top).contains(&x) {
                    break;
                }
                open.pop();
                self.attached_use_receivers(inst.objects[top].components[0], &mut out);
            }
            open.push(x);
            if !inst.objects[x].active {
                continue;
            }
            for (k, (_, c)) in inst.prefab.objects[x].components.iter().enumerate() {
                let crate::assets::db::Component::Script { class, .. } = c else { continue };
                // PhysicalBehaviour.Use only invokes ModdingApi.Events.OnItemActivated (no mods).
                if !USE_RECEIVERS.contains(&class.as_str()) || class == "PhysicalBehaviour" {
                    continue;
                }
                if class == "UseEventTrigger" {
                    out.push(UseReceiver::Event(inst.objects[x].components[k]));
                    continue;
                }
                match self.behaviour_of.get(&inst.objects[x].components[k]) {
                    Some(&(_, b @ (Behaviour::Firearm(_) | Behaviour::Glowtube(_) | Behaviour::Led(_) | Behaviour::Detector(_) | Behaviour::Explosive(_) | Behaviour::Launcher(_) | Behaviour::Bell(_) | Behaviour::MachineGun(_) | Behaviour::Minigun(_) | Behaviour::Generator(_) | Behaviour::Floodlight(_) | Behaviour::SpikeGrenade(_) | Behaviour::HeatingElement(_) | Behaviour::Bulb(_) | Behaviour::Thrusterbed(_) | Behaviour::Piston(_) | Behaviour::Car(_) | Behaviour::Syringe(_) | Behaviour::JetEngine(_) | Behaviour::Chainsaw(_) | Behaviour::Emp(_) | Behaviour::Decimator(_) | Behaviour::ImmobilityField(_) | Behaviour::EnergySword(_) | Behaviour::SignalConverter(_) | Behaviour::Metronome(_) | Behaviour::ActivatorElectrode(_) | Behaviour::Thermometer(_) | Behaviour::InfraredThermometer(_) | Behaviour::Blaster(_) | Behaviour::Beamformer(_) | Behaviour::Scifi40(_) | Behaviour::HealthGun(_) | Behaviour::FreezeGun(_) | Behaviour::Flare(_) | Behaviour::Display(_) | Behaviour::Laser(_) | Behaviour::LaserTurret(_) | Behaviour::FirearmAttachment(_) | Behaviour::BatteryAttachment(_) | Behaviour::Television(_) | Behaviour::UniversalBell(_) | Behaviour::Valve(_) | Behaviour::TimedGate(_) | Behaviour::Defibrillator(_) | Behaviour::CardiopulmonaryBypass(_) | Behaviour::HeartMonitor(_) | Behaviour::Excalibur(_) | Behaviour::BloodTank(_) | Behaviour::ArchelixCaster(_) | Behaviour::Grip(_) | Behaviour::TeslaCoil(_) | Behaviour::AtomBomb(_) | Behaviour::SeismicCharge(_))))=> out.push(UseReceiver::Behaviour(b)),
                    Some(&(_, b @ Behaviour::Translated(t))) if t.has_use() => out.push(UseReceiver::Behaviour(b)),
                    _ => unimplemented!("{class}.Use"),
                }
            }
        }
        while let Some(top) = open.pop() {
            self.attached_use_receivers(inst.objects[top].components[0], &mut out);
        }
        out
    }

    /// The Use receivers of the clones parented under the Transform `key` (Instantiate(.., parent), or a re-parented body).
    fn attached_use_receivers(&mut self, key: i32, out: &mut Vec<UseReceiver>) {
        for c in self.attached_clones.get(&key).cloned().unwrap_or_default() {
            if !self.destroyed_components.contains(&c.id) {
                let below = self.with_component_instance(c.id, |g, clone, o| g.use_receivers_below(clone, o));
                out.extend(below);
            }
        }
    }

    pub(super) fn broadcast_use(&mut self, p: usize, act: &Act) {
        let id = self.physicals[p].id;
        let receivers: Vec<UseReceiver> = self.with_component_instance(id, |g, inst, start| g.use_receivers_below(inst, start));
        for b in receivers {
            let b = match b {
                UseReceiver::Event(id) => {
                    self.use_event_trigger_use(id);
                    continue;
                }
                UseReceiver::Behaviour(b) => b,
            };
            match b {
                Behaviour::Firearm(f) => self.firearm_use(f),
                Behaviour::Grip(g) => self.grip_use(g),
                Behaviour::Explosive(e) => self.explosive_use(e),
                Behaviour::Launcher(l) => self.launcher_use(l),
                Behaviour::Bell(b) => self.bell_use(b),
                Behaviour::UniversalBell(b) => self.universal_bell_use(b),
                // The activation's channel.
                Behaviour::Valve(v) => self.valve_use(v, act.channel as i32),
                Behaviour::TimedGate(g) => self.timed_gate_use(g),
                Behaviour::Defibrillator(d) => self.defibrillator_use(d),
                Behaviour::CardiopulmonaryBypass(c) => self.cardiopulmonary_bypass_use(c),
                Behaviour::HeartMonitor(h) => self.heart_monitor_use(h),
                Behaviour::Excalibur(e) => self.excalibur_use(e),
                Behaviour::BloodTank(t) => self.blood_tank_use(t, act.channel as i32),
                Behaviour::ArchelixCaster(c) => self.archelix_caster_use(c),
                Behaviour::MachineGun(m) => self.machine_gun_use(m),
                Behaviour::Minigun(m) => self.minigun_use(m),
                Behaviour::Translated(t) => self.tr_use(t, super::translated::rt::Activation { Channel: act.channel as i32 }),
                Behaviour::Glowtube(g) => self.glowtube_use(g),
                Behaviour::Led(i) => self.led_use(i),
                Behaviour::Detector(i) => self.detector_use(i),
                Behaviour::Generator(i) => self.generator_use(i),
                Behaviour::Floodlight(i) => self.floodlight_use(i),
                Behaviour::SpikeGrenade(i) => self.spike_grenade_use(i),
                Behaviour::HeatingElement(i) => self.heating_element_use(i),
                Behaviour::Bulb(i) => self.bulb_use(i),
                Behaviour::Thrusterbed(i) => self.thrusterbed_use(i),
                Behaviour::Piston(i) => self.piston_use(i),
                Behaviour::Car(i) => self.car_use(i, act.channel as i32),
                Behaviour::Syringe(i) => self.syringe_use(i, act.channel as i32),
                Behaviour::JetEngine(i) => self.jet_engine_use(i),
                // ChainsawBehaviour.Use: PlayClipOnce(Rev) when not destroyed (audio).
                Behaviour::Chainsaw(_) => {}
                Behaviour::Emp(i) => self.emp_use(i),
                Behaviour::LaserTurret(i) => self.laser_turret_use(i),
                Behaviour::Decimator(i) => self.decimator_use(i),
                Behaviour::ImmobilityField(i) => self.immobility_field_use(i),
                // TeslaCoilBehaviour.Use() takes no parameter; SendMessage calls it all the same.
                Behaviour::TeslaCoil(i) => self.tesla_coil_use(i),
                Behaviour::AtomBomb(i) | Behaviour::SeismicCharge(i) => self.prefab_bomb_use(i),
                Behaviour::EnergySword(i) => self.energy_sword_use(i),
                // MetronomeBehaviour.Use: `base.enabled && !activation.Contains(base.transform.root)`.
                Behaviour::Metronome(m) => {
                    if !act.contains(self.root_id(self.metronomes[m].phys)) {
                        self.metronome_use(m)
                    }
                }
                Behaviour::ActivatorElectrode(e) => self.activator_electrode_use(e, act),
                Behaviour::Thermometer(t) => self.thermometer_use(t),
                Behaviour::Flare(f) => self.flare_use(f),
                Behaviour::Display(d) => self.display_use(d),
                Behaviour::Laser(l) => self.laser_use(l),
                Behaviour::Television(t) => self.television_use(t),
                Behaviour::FirearmAttachment(a) | Behaviour::BatteryAttachment(a) => self.firearm_attachment_use(a, p),
                Behaviour::InfraredThermometer(i) => self.infrared_use(i),
                Behaviour::Blaster(i) => self.blaster_use(i),
                Behaviour::Beamformer(i) => self.beamformer_use(i),
                Behaviour::Scifi40(i) => self.scifi40_use(i),
                Behaviour::HealthGun(i) | Behaviour::FreezeGun(i) => self.effect_gun_use(i),
                // The activation's Direct flag and channel.
                Behaviour::SignalConverter(i) => self.signal_converter_use(i, act.direct, act.channel),
                _ => unreachable!(),
            }
        }
    }

    /// ToolControllerBehaviour.DetermineHovering. currentlyHovering was just cleared, so the held object's own
    /// check never matches.
    fn determine_hovering(&mut self) {
        let mask = self.tool.as_ref().unwrap().SelectionLayer;
        let hits = self.overlap_point(self.MousePosition, mask, -0.9, 0.9);
        let mut best = None;
        let mut least = f32::MAX;
        for ph in hits {
            let p = &self.physicals[ph];
            let v = match self.prefs.ToolSelectionMode {
                // PhysicalBehaviour.ObjectAreaByMass
                SELECTION_AREA => p.rigidbody.mass(&self.world) / p.TrueInitialMass * p.ObjectArea,
                SELECTION_MASS => p.InitialMass,
                _ => unimplemented!("ToolSelectionMode.Depth: sprite sorting layers"),
            };
            if v < least {
                least = v;
                best = Some(ph);
            }
        }
        let t = self.tool.as_mut().unwrap();
        t.currentlyHovering = best;
        if t.hover != t.currentlyHovering {
            match t.currentlyHovering {
                // SelectionController.SetHovering (RefreshOutlines draws outlines).
                Some(h) if self.physicals[h].Selectable => t.hover = Some(h),
                _ if t.hover.is_some() => t.hover = None,
                _ => {}
            }
        }
    }

    /// SelectionController.Select(PhysicalBehaviour, multiple); the modding events have no listeners.
    fn select(&mut self, ph: usize, multiple: bool) {
        if !self.physicals[ph].Selectable {
            return;
        }
        let t = self.tool.as_mut().unwrap();
        if !multiple {
            t.rawSelection.clear();
        }
        if !t.rawSelection.contains(&ph) {
            t.rawSelection.push(ph);
        }
    }

    /// ToolControllerBehaviour.HandleTools (Global.main.UILock is never set).
    fn handle_tools(&mut self) {
        let fast = self.input.held("fast");
        if self.input.down("drag") {
            let t = self.tool.as_ref().unwrap();
            match t.currentlyHovering {
                Some(h) if fast && t.rawSelection.contains(&h) => {
                    // SelectionController.Deselect
                    let t = self.tool.as_mut().unwrap();
                    t.rawSelection.retain(|&p| p != h);
                    if Some(h) == t.ActiveSingleSelected {
                        self.current_on_deselect();
                        let t = self.tool.as_mut().unwrap();
                        if !t.rawSelection.is_empty() {
                            t.ActiveSingleSelected = Some(t.rawSelection[0]);
                        }
                    }
                }
                Some(h) => {
                    let multiple = fast || t.rawSelection.contains(&h);
                    self.select(h, multiple);
                    if self.tool.as_ref().unwrap().ActiveSingleSelected.is_some() {
                        self.current_on_deselect();
                    }
                    self.tool.as_mut().unwrap().ActiveSingleSelected = Some(h);
                }
                None => {
                    self.escape_and_deselect();
                    // DragTool.UsesEmptyDrag is false: StartSelectionDrag (the wire tools' is true).
                    let t = self.tool.as_mut().unwrap();
                    if !self.wire_tool.chosen && !t.isSelecting {
                        let Some(screen) = self.input.screen else { unimplemented!("StartSelectionDrag: Input.mousePosition in a recording") };
                        t.isSelecting = true;
                        t.selectionDragStart = screen.mouse;
                    }
                }
            }
            self.tool.as_mut().unwrap().isCurrentlyHolding = true;
            self.current_on_select();
        }
        if self.input.down("context") && self.tool.as_ref().unwrap().currentlyHovering.is_some() {
            let t = self.tool.as_ref().unwrap();
            let h = t.currentlyHovering.unwrap();
            let multiple = fast || t.rawSelection.contains(&h);
            self.select(h, multiple);
        }
        if self.input.held("drag") && self.tool.as_ref().unwrap().isCurrentlyHolding {
            self.current_on_hold();
        }
        // The EventSystem's selected object is never part of the context menu.
        if self.input.up("drag") {
            self.escape_and_deselect();
            self.stop_selection_drag(fast);
        }
    }

    /// ToolControllerBehaviour.StopSelectionDrag: a box dragged past √200 px selects what it overlaps (replacing
    /// the selection unless "fast" is held). The box UI is drawn by the window.
    fn stop_selection_drag(&mut self, fast: bool) {
        let t = self.tool.as_mut().unwrap();
        if !t.isSelecting {
            return;
        }
        t.isSelecting = false;
        let Some(screen) = self.input.screen else { unimplemented!("StopSelectionDrag: Input.mousePosition in a recording") };
        let start = t.selectionDragStart;
        let a = screen.to_world(start);
        let d = [start[0] - screen.mouse[0], start[1] - screen.mouse[1]];
        if d[0] * d[0] + d[1] * d[1] < 200.0 {
            return;
        }
        if !fast {
            t.rawSelection.clear();
        }
        let mask = t.SelectionLayer;
        let hits = self.overlap_area_all(a, [self.MousePosition[0], self.MousePosition[1]], mask);
        // SelectionController.Select(IEnumerable, fast)
        let selectable: Vec<bool> = hits.iter().map(|&p| self.physicals[p].Selectable).collect();
        let t = self.tool.as_mut().unwrap();
        if !fast {
            t.rawSelection.clear();
        }
        for (p, ok) in hits.into_iter().zip(selectable) {
            if ok && !t.rawSelection.contains(&p) {
                t.rawSelection.push(p);
            }
        }
    }

    /// ToolControllerBehaviour.EscapeAndDeselect (the pointer is never over a NoDeselectUi).
    fn escape_and_deselect(&mut self) {
        self.current_on_deselect();
        let fast = self.input.held("fast");
        let t = self.tool.as_mut().unwrap();
        t.ActiveSingleSelected = None;
        t.isCurrentlyHolding = false;
        let kept = t.currentlyHovering.is_some_and(|h| t.rawSelection.contains(&h));
        if !kept && !fast {
            t.rawSelection.clear();
        }
    }

    /// CurrentTool.OnSelect / OnHold / OnDeselect: the DragTool, or the BloodWireTool after SetToolByTypeName.
    fn current_on_select(&mut self) {
        if self.wire_tool.chosen {
            self.wire_on_select();
        } else {
            self.drag_on_select();
        }
    }

    fn current_on_hold(&mut self) {
        if self.wire_tool.chosen {
            self.wire_on_hold();
        } else {
            self.drag_on_hold();
        }
    }

    fn current_on_deselect(&mut self) {
        if self.wire_tool.chosen {
            self.wire_on_deselect();
        } else {
            self.drag_on_deselect();
        }
    }

    /// ToolControllerBehaviour.SetToolByTypeName (-> SetTool): the old tool is unchosen and deselected, then the new one is
    /// found or added to the ToolController and chosen. The DragTool and the BloodWireTool are the tools rewritten.
    pub fn set_tool_by_type_name(&mut self, name: &str) {
        if !matches!(name, "DragTool" | "BloodWireTool") {
            unimplemented!("ToolControllerBehaviour.SetToolByTypeName({name})");
        }
        if self.wire_tool.chosen {
            // DistanceWireTool.OnToolUnchosen is empty.
        } else {
            // DragTool.OnToolUnchosen (objectsToMoveInPauseMode stays empty: nothing is dragged while paused).
            let t = self.tool.as_mut().unwrap();
            t.isActive = false;
            t.shouldSendContinuousUseMessage = false;
        }
        self.tool.as_mut().unwrap().isCurrentlyHolding = false;
        self.current_on_deselect();
        self.tool.as_mut().unwrap().ActiveSingleSelected = None;
        self.wire_tool.chosen = false;
        if name == "BloodWireTool" {
            self.wire_tool_choose();
        } else {
            self.tool.as_mut().unwrap().isActive = true;
        }
    }

    /// Rigidbody2D.mass = v on the tool (Rigidbody2D::SetMass 180c23790): clamped, then b2Body::ResetMassData.
    fn tool_set_mass(&mut self, v: f32) {
        let body = self.tool.as_ref().unwrap().body;
        self.body_set_mass(body, v);
    }

    /// DragTool.OnSelect (never paused; nothing in the scene is Undraggable).
    fn drag_on_select(&mut self) {
        let Some(sel) = self.tool.as_ref().unwrap().ActiveSingleSelected else { return };
        let mouse = self.MousePosition;
        let t = self.tool.as_mut().unwrap();
        t.controlRotation = false;
        t.position = mouse;
        let body = t.body;
        // Rigidbody2D::SetPosition (180c23cb0): b2Body::SetTransform at the body's angle, then awake.
        let clamp = |v: f32| if -1_000_000.0 <= v && v <= 1_000_000.0 { v } else if -1_000_000.0 <= v { 1_000_000.0 } else { -1_000_000.0 };
        let angle = self.world.body(body).sweep.a;
        self.world.set_transform(body, V::new(clamp(mouse[0]), clamp(mouse[1])), angle);
        self.world.wake_body(body);
        // Rigidbody2D::SetVelocity with zero: no wake.
        self.world.body_mut(body).linear_velocity = V::ZERO;
        let local = self.inverse_transform_point(sel, mouse);
        let mass = self.physicals[sel].rigidbody.mass(&self.world) * 10.0;
        self.tool_set_mass(mass);
        let rotation = dump::euler_z(self.physicals[sel].transform.rotation);
        let snapping = self.input.held("snap");
        let prev = self.mouse_pos_with_snap();
        let t = self.tool.as_mut().unwrap();
        t.offset = [local[0], local[1]];
        t.currentRotation = rotation;
        t.initialSnappingFinished = !snapping;
        // Rigidbody2D::SetDrag / SetAngularDrag: the b2 damping, no wake.
        let b = self.world.body_mut(body);
        b.linear_damping = 150.0;
        b.angular_damping = 150.0;
        t.selectedRigidbody = Some(sel);
        t.prevMousePos = prev;
        self.destroy_drag_joint();
        self.add_drag_joint(sel);
    }

    /// Object.Destroy(dragJoint): DestroyObjectDelayed's DelayedCallManager entry at min(fixedTimeAsDouble, time).
    fn destroy_drag_joint(&mut self) {
        let at = self.time.fixedTimeAsDouble.min(self.time.timeAsDouble);
        if let Some(j) = self.tool.as_mut().unwrap().dragJoint.as_mut() {
            j.destroy_at.get_or_insert(at);
        }
    }

    /// AddComponent<HingeJoint2D>() and the setters OnSelect runs on it, ending with connectedBody = the selected
    /// limb. HingeJoint2D::Reset gives anchor (0,0), autoConfigureConnectedAnchor, motor 0°/s at 10000, useMotor
    /// and useLimits off, limits 0..359°, infinite break force and torque. AddToManager creates the joint on the
    /// scene's ground body; useMotor = false finds it already off; autoConfigureConnectedAnchor and connectedAnchor
    /// each ReCreate it there and connectedBody builds it again (Cleanup(true): a fresh reference angle). The
    /// ground joints leave behind the tool woken by b2World::DestroyJoint. The last HingeJoint2D::Create:
    /// localAnchorA = anchor × lossyScale, localAnchorB = connectedAnchor × the limb's lossyScale, referenceAngle
    /// = body B angle − body A angle; FinalizeCreateJoint wakes both; infinite breaking stays off the break list.
    fn add_drag_joint(&mut self, sel: usize) {
        let mut joint = DragJoint { joint: None, motorSpeed: 0.0, maxMotorTorque: 10000.0, useMotor: false, destroy_at: None };
        let t = self.tool.as_ref().unwrap();
        let (a, offset) = (t.body, t.offset);
        let ph = &self.physicals[sel];
        // HingeJoint2D::Create returns early on an inactive connected object.
        if ph.active && ph.rigidbody.detached.is_none() {
            let b = ph.rigidbody.body;
            let s = self.lossy_scale(sel);
            let def = RevoluteDef {
                local_anchor_a: V::ZERO,
                local_anchor_b: V::new(s[0] * offset[0], s[1] * offset[1]),
                reference_angle: self.world.body(b).sweep.a - self.world.body(a).sweep.a,
                enable_limit: false,
                lower_angle: 0.0,
                upper_angle: 359.0 * DEG2RAD,
                enable_motor: false,
                motor_speed: 0.0,
                max_motor_torque: 10000.0,
            };
            self.world.wake_body(a);
            let id = self.world.create_joint(def, a, b, false);
            self.world.wake_joined(a);
            self.world.wake_joined(b);
            joint.joint = Some(id);
        }
        let t = self.tool.as_mut().unwrap();
        if let Some(old) = t.dragJoint.replace(joint) {
            if old.destroy_at.is_some() {
                t.destroying.push(old);
            }
        }
    }

    /// DragTool.OnHold (never paused, no UI lock).
    fn drag_on_hold(&mut self) {
        let t = self.tool.as_mut().unwrap();
        t.shouldSendContinuousUseMessage = false;
        let (Some(sel), Some(_)) = (t.ActiveSingleSelected, t.selectedRigidbody) else { return };
        // (bool)ActiveSingleSelected: a destroyed object reads null.
        if self.physicals[sel].destroyed {
            return;
        }
        if self.input.down("snap") {
            self.tool.as_mut().unwrap().initialSnappingFinished = false;
        }
        let flag = !self.tool.as_ref().unwrap().initialSnappingFinished;
        let snapped = self.mouse_pos_with_snap();
        if flag {
            let pos = self.physicals[sel].transform.position;
            let d = sub3(self.snap_to_grid(pos), pos);
            let t = self.tool.as_mut().unwrap();
            t.initialTranslationSnapOffset = d;
            t.snappedMouseDelta = d;
            t.initialSnappingFinished = true;
        } else {
            let t = self.tool.as_mut().unwrap();
            t.snappedMouseDelta = sub3(snapped, t.prevMousePos);
        }
        self.tool.as_mut().unwrap().prevMousePos = snapped;
        if self.input.down("right") || self.input.down("left") {
            self.tool.as_mut().unwrap().rotationSpeed = 1.0;
        }
        if self.input.held("right") || self.input.held("left") {
            self.tool.as_mut().unwrap().rotationSpeed += self.time.unscaledDeltaTime * 15.0;
        }
        self.move_to_mouse();
        if self.input.down("activateDirect") {
            for p in self.tool.as_ref().unwrap().rawSelection.clone() {
                if !self.physicals[p].destroyed {
                    self.activate_direct(p);
                }
            }
        }
        // The UseContinuous broadcast is ContinuousActivationBehaviour's (continuous.rs).
        self.tool.as_mut().unwrap().shouldSendContinuousUseMessage = self.input.held("activateDirect");
        if self.input.held("delete") {
            self.user_delete_selection();
        }
    }

    /// SelectionController.UserDeleteSelection: each live, Deletable object of the selection gets OnUserDelete on its
    /// root GameObject, then Object.Destroy(root); the names (once each) are announced.
    pub fn user_delete_selection(&mut self) {
        let selected = self.tool.as_ref().unwrap().rawSelection.clone();
        if selected.is_empty() {
            return;
        }
        // HashSet<string> enumerates in insertion order while nothing is removed.
        let mut names: Vec<String> = Vec::new();
        for p in selected {
            if self.physicals[p].destroyed || !self.physicals[p].Deletable {
                continue;
            }
            let root = self.root_id(p);
            self.send_on_user_delete(root);
            self.destroy_object(root);
            let name = self.object_name(p, root);
            if !names.contains(&name) {
                names.push(name);
            }
        }
        for name in names {
            self.notify(format!("<b>{}</b> removed", name.to_uppercase()));
        }
    }

    /// transform.root.gameObject.name of a physical object whose root Transform is `root`.
    pub(super) fn object_name(&mut self, p: usize, root: i32) -> String {
        if let Some(n) = self.object_names.get(&root) {
            return n.clone();
        }
        if self.physicals[p].transform_id == root {
            return self.physicals[p].name.clone();
        }
        self.with_component_instance(root, |_, inst, o| inst.prefab.objects[o].name.clone())
    }

    /// SendMessage("OnUserDelete", DontRequireReceiver) on a GameObject (by its Transform id): its own components only.
    /// Messages.IOnUserDelete is declared by ActOnUserDelete (on no catalog item) and by the Hover wires, pins and
    /// links, which the rewrite refuses at spawn.
    fn send_on_user_delete(&mut self, root: i32) {
        let receivers = self.with_component_instance(root, |_, inst, o| {
            let active = inst.objects[o].active;
            inst.prefab.objects[o].components.iter().filter(|(_, c)| matches!(c, crate::assets::db::Component::Script { class, .. } if class == "ActOnUserDelete")).count() * active as usize
        });
        if receivers > 0 {
            unimplemented!("ActOnUserDelete.OnUserDelete");
        }
    }

    /// DragTool.Update (no dialog, UI block, trigger editor or pause menu is ever open): copy, paste and flip.
    pub fn drag_tool_update(&mut self) {
        let Some(t) = self.tool.as_ref() else { return };
        if !t.isActive {
            return;
        }
        // ClipboardControllerBehaviour.ObjectStateInClipboard stays null: copy is the only thing that fills it.
        if self.input.down("copy") && !t.rawSelection.is_empty() {
            unimplemented!("DragTool.Update: copy (new Clipboard: ObjectStateConverter.Convert, ContraptionOutlineSerialiser.GenerateOutline)");
        }
        if self.input.down("flip") {
            self.flip_selection();
        }
    }

    /// SelectionController.FlipSelection
    fn flip_selection(&mut self) {
        if self.tool.as_ref().unwrap().rawSelection.is_empty() {
            return;
        }
        unimplemented!("SelectionController.FlipSelection: Transform.localScale of a live hierarchy, OnPreFlip / OnFlip");
    }

    /// DragTool.MoveToMouse: Rigidbody2D.AddForce (180c1bf80, ForceMode2D.Force) wakes the body and adds the force.
    fn move_to_mouse(&mut self) {
        let target = if self.input.held("snap") { self.mouse_pos_with_snap() } else { self.MousePosition };
        let t = self.tool.as_ref().unwrap();
        let v = sub3(target, t.position);
        let body = t.body;
        let k = self.time.deltaTime * 60.0 * 1950.0 * self.world.body(body).rigidbody.mass;
        self.world.body_mut(body).apply_force_to_center(V::new(k * v[0], k * v[1]));
    }

    /// DragTool.OnFixedHold (never paused; nothing is Undraggable).
    fn drag_on_fixed_hold(&mut self) {
        let t = self.tool.as_ref().unwrap();
        if t.ActiveSingleSelected.is_none_or(|p| self.physicals[p].destroyed) || t.selectedRigidbody.is_none() {
            return;
        }
        let d = sub3(self.MousePosition, t.position);
        // Vector3.Distance
        if ((d[0] * d[0] + d[1] * d[1]) + d[2] * d[2]).sqrt() < 1.0 {
            // Rigidbody2D.velocity *= GetDragFunction(): SetVelocity wakes only for a non-zero velocity.
            let b = self.world.body_mut(t.body);
            let v = b.linear_velocity;
            b.set_linear_velocity(V::new(v.x * 0.9, v.y * 0.9));
        }
        self.drag_rotate();
    }

    /// DragTool.Rotate
    fn drag_rotate(&mut self) {
        let (left, right, fast) = (self.input.held("left"), self.input.held("right"), self.input.held("fast"));
        let t = self.tool.as_mut().unwrap();
        let Some(j) = t.dragJoint.as_mut() else { return };
        let (mut speed, mut torque) = (j.motorSpeed, j.maxMotorTorque);
        if t.controlRotation {
            torque = 1000.0;
            speed = 0.0;
        }
        let mut use_motor = None;
        if left {
            let num = if fast { 1000.0 } else { 250.0 };
            t.controlRotation = true;
            use_motor = Some(true);
            speed = num * (t.rotationSpeed * 0.1);
        }
        if right {
            let num = if fast { -1000.0 } else { -250.0 };
            t.controlRotation = true;
            use_motor = Some(true);
            speed = num * (t.rotationSpeed * 0.1);
        }
        let control = t.controlRotation;
        if let Some(on) = use_motor {
            self.drag_set_use_motor(on);
        }
        self.drag_set_motor(speed, torque);
        if !control {
            self.drag_set_use_motor(false);
        }
    }

    /// HingeJoint2D.useMotor = on (180bfdda0): b2RevoluteJoint::EnableMotor, which wakes both ends on a change.
    fn drag_set_use_motor(&mut self, on: bool) {
        let j = self.tool.as_mut().unwrap().dragJoint.as_mut().unwrap();
        j.useMotor = on;
        if let Some(id) = j.joint {
            self.revolute_enable_motor(id, on);
        }
    }

    fn revolute_enable_motor(&mut self, id: JointId, on: bool) {
        if self.world.revolute(id).def.enable_motor == on {
            return;
        }
        self.wake_joint_bodies(id);
        self.world.revolute_mut(id).def.enable_motor = on;
    }

    fn wake_joint_bodies(&mut self, id: JointId) {
        let j = self.world.joints[id].as_ref().unwrap();
        let (a, b) = (j.body_a, j.body_b);
        self.world.wake_body(a);
        self.world.wake_body(b);
    }

    /// HingeJoint2D.motor = (speed, torque) (180bfdc90): clamped, useMotor forced on, then EnableMotor,
    /// SetMotorSpeed (radians) and SetMaxMotorTorque, each waking both ends only on a change.
    fn drag_set_motor(&mut self, speed: f32, torque: f32) {
        let speed = if -1_000_000.0 <= speed && speed <= 1_000_000.0 { speed } else if -1_000_000.0 <= speed { 1_000_000.0 } else { -1_000_000.0 };
        let torque = if 0.0 <= torque && torque <= 1_000_000.0 { torque } else if 0.0 <= torque { 1_000_000.0 } else { 0.0 };
        let j = self.tool.as_mut().unwrap().dragJoint.as_mut().unwrap();
        (j.motorSpeed, j.maxMotorTorque, j.useMotor) = (speed, torque, true);
        let Some(id) = j.joint else { return };
        self.revolute_enable_motor(id, true);
        let rad = speed * DEG2RAD;
        if self.world.revolute(id).def.motor_speed != rad {
            self.wake_joint_bodies(id);
            self.world.revolute_mut(id).def.motor_speed = rad;
        }
        if self.world.revolute(id).def.max_motor_torque != torque {
            self.wake_joint_bodies(id);
            self.world.revolute_mut(id).def.max_motor_torque = torque;
        }
    }

    /// DragTool.OnDeselect
    fn drag_on_deselect(&mut self) {
        let t = self.tool.as_mut().unwrap();
        t.shouldSendContinuousUseMessage = false;
        t.controlRotation = false;
        self.destroy_drag_joint();
        self.tool_set_mass(9500.0);
    }

    /// The tool's queued HingeJoint2D destroys due by now (DelayedCallManager times).
    pub fn tool_due_destroys(&self, now: f64) -> Vec<f64> {
        let Some(t) = self.tool.as_ref() else { return Vec::new() };
        t.destroying.iter().chain(&t.dragJoint).filter_map(|j| j.destroy_at).filter(|&at| at <= now).collect()
    }

    /// The delayed destroy of the drag joint queued at `at`: Joint2D::Cleanup's b2World::DestroyJoint; the
    /// `dragJoint` field then reads null.
    pub fn tool_run_destroy(&mut self, at: f64) {
        let t = self.tool.as_mut().unwrap();
        let joint = if t.dragJoint.as_ref().is_some_and(|j| j.destroy_at == Some(at)) {
            t.dragJoint.take().unwrap()
        } else {
            let i = t.destroying.iter().position(|j| j.destroy_at == Some(at)).unwrap();
            t.destroying.remove(i)
        };
        if let Some(id) = joint.joint {
            self.world.destroy_joint(id);
        }
    }

    /// The tool body's pose to its Transform after a step (awake, no interpolation, rotation fixed about z).
    pub fn tool_write_back(&mut self) {
        let Some(t) = self.tool.as_mut() else { return };
        let b = self.world.body(t.body);
        if b.flags & crate::b2::world::BODY_AWAKE == 0 {
            return;
        }
        let half = b.sweep.a * 0.5;
        t.position = [b.xf.p.x, b.xf.p.y, t.position[2]];
        if b.sweep.a != 0.0 {
            unimplemented!("ToolController body turned: Transform rotation write-back");
        }
        let _ = half;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::{new_game, step};

    fn ctl(g: &mut Game, key: &str, down: bool, held: bool, up: bool) {
        g.input.controls.insert(key.into(), Control { down, held, up });
    }

    #[test]
    fn disintegrating_a_body_cleans_up_its_prefab_joints() {
        let Some((mut g, first)) = spawned("Car") else { return };
        // A part of the car whose body holds a prefab-cloned joint (its WheelJoint2Ds).
        let (p, slot) = (first..g.physicals.len())
            .find_map(|p| {
                let body = g.physicals[p].rigidbody.body;
                let joints = g.world.body(body).joints.clone();
                joints.iter().find_map(|&(_, j)| g.anchored_joints.iter().position(|a| a.joint == Some(j))).map(|s| (p, s))
            })
            .expect("a car body with a prefab joint");
        g.physical_disintegrate(p);
        assert!(g.anchored_joints[slot].joint.is_none());
        frames(&mut g, 60);
    }

    fn frames(g: &mut Game, n: usize) {
        for _ in 0..n {
            step(g, 1.0 / 60.0).unwrap();
            for c in g.input.controls.values_mut() {
                c.down = false;
                c.up = false;
            }
        }
    }

    /// A Default map game with `item` spawned (the way a "spawnRight" release does) and settled. Returns None
    /// without the installed game.
    fn spawned(item: &str) -> Option<(Game, usize)> {
        let game = crate::game_assets::find_game(None).ok()?;
        let mut g = new_game(&game, "Default", Some(item), None, 0).unwrap();
        frames(&mut g, 30);
        let first = g.physicals.len();
        let c = g.BoundingBox.center;
        let ground = g.raycast_first(crate::b2::math::Vec2::new(c[0], c[1]), [0.0, -1.0], 1.0e4)?.point.y;
        g.input.mouse = [c[0], ground + 0.6, 0.0];
        ctl(&mut g, "spawnRight", false, false, true);
        frames(&mut g, 1);
        ctl(&mut g, "spawnRight", false, false, false);
        assert!(g.physicals.len() > first, "{item}: nothing spawned");
        // It lands and comes to rest, so the mouse stays on it.
        frames(&mut g, 300);
        Some((g, first))
    }

    /// Picks the object up with the mouse on a point of its collider, then holds "delete" for `n` frames. Returns the
    /// physical the click selected (the one on top where an item has several).
    fn grab_and_delete(g: &mut Game, p: usize, n: usize) -> usize {
        // A point the object's collider contains: its Transform, its centre of mass or a collider's box centre.
        let mask = g.tool.as_ref().unwrap().SelectionLayer;
        let body = g.physicals[p].rigidbody.body;
        let mut points = vec![g.physicals[p].transform.position, {
            let c = g.world.body(body).sweep.c;
            [c.x, c.y, 0.0]
        }];
        for f in g.world.body(body).fixtures.clone() {
            let fx = g.world.fixture(f);
            let b = fx.shape.compute_aabb(&g.world.body(fx.body).xf, 0);
            points.push([(b.lower.x + b.upper.x) * 0.5, (b.lower.y + b.upper.y) * 0.5, 0.0]);
        }
        let c = points.into_iter().find(|&q| !g.overlap_point(q, mask, -0.9, 0.9).is_empty()).expect("a point inside the object");
        g.input.mouse = c;
        // Global.MousePosition follows Input.mousePosition at the end of the Update scripts.
        frames(g, 1);
        ctl(g, "drag", true, true, false);
        frames(g, 2);
        ctl(g, "drag", false, true, false);
        let sel = g.tool.as_ref().unwrap().ActiveSingleSelected.expect("the object was not picked up");
        ctl(g, "delete", true, true, false);
        frames(g, n);
        ctl(g, "delete", false, false, true);
        sel
    }

    #[test]
    fn delete_destroys_the_selected_object_through_object_destroy() {
        for item in ["Human", "Flask", "Pistol", "Generator", "Fan", "Handgrenade", "Piston"] {
            let Some((mut g, first)) = spawned(item) else { return };
            assert!(g.undo.history.len() == 1, "{item}");
            let p = grab_and_delete(&mut g, first, 3);
            let (id, root) = (g.physicals[p].id, g.root_id(p));
            // Object.Destroy ran: the component reads null, OnDestroy dropped the undo entry and the object left the loops.
            assert!(g.physicals[p].destroyed && !g.physicals[p].enabled, "{item}: still alive");
            // The whole root GameObject went: every physical object of the item, and a person with its limbs and joints.
            assert!(g.physicals[first..].iter().all(|q| q.destroyed), "{item}: a part survived");
            assert!(g.people.iter().all(|q| q.destroyed) && g.hinges.iter().all(|h| h.joint.is_none()), "{item}: person");
            assert!(g.destroyed_components.contains(&id) && g.destroyed_components.contains(&root), "{item}");
            assert!(g.undo.history.is_empty() && g.undo.currentHistoryPosition == -1, "{item}: DeregisterBehaviour.OnDestroy");
            assert!(g.notifications.contains(&format!("<b>{}</b> removed", item.to_uppercase())), "{item}: {:?}", g.notifications);
            // The held object is gone: the drag joint went with its body and the next frames run on.
            frames(&mut g, 30);
        }
    }

    #[test]
    fn clear_living_and_clear_mess() {
        let Some((mut g, first)) = spawned("Human") else { return };
        g.clear_debris();
        g.clear_living();
        frames(&mut g, 3);
        assert!(g.physicals[first..].iter().all(|q| q.destroyed) && g.people.iter().all(|p| p.destroyed));
        assert!(g.undo.history.is_empty());
        assert_eq!(g.notifications, ["Cleared debris", "Cleared living"]);
        frames(&mut g, 30);
    }

    #[test]
    fn undo_destroys_the_last_spawn() {
        let Some((mut g, first)) = spawned("Flask") else { return };
        ctl(&mut g, "undo", false, false, true);
        frames(&mut g, 2);
        assert!(g.physicals[first].destroyed);
        assert!(g.notifications.contains(&"<b>FLASK</b> removed".to_string()));
        assert!(g.undo.history.is_empty());
    }

    #[test]
    fn bypass_machine_use_shows_and_hides_its_cosmetics() {
        let Some((mut g, first)) = spawned("Cardiopulmonary Bypass Machine") else { return };
        let (id, cosmetics) = (g.cardiopulmonary_bypasses[0].id, g.cardiopulmonary_bypasses[0].Cosmetics);
        let shown = |g: &mut Game| g.with_component_instance(id, |_, inst, _| inst.objects[cosmetics].active);
        // Start: Activated is false, so Cosmetics is hidden.
        assert!(!g.cardiopulmonary_bypasses[0].Activated && !shown(&mut g));
        g.broadcast_use(first, &Act::direct(0));
        assert!(g.cardiopulmonary_bypasses[0].Activated && shown(&mut g));
        g.broadcast_use(first, &Act::direct(0));
        assert!(!g.cardiopulmonary_bypasses[0].Activated && !shown(&mut g));
    }

    /// A point on a physical's collider that the selection mask overlaps.
    fn point_on(g: &mut Game, p: usize) -> Vec<[f32; 3]> {
        let mask = g.tool.as_ref().unwrap().SelectionLayer;
        let body = g.physicals[p].rigidbody.body;
        let mut points = vec![g.physicals[p].transform.position];
        for f in g.world.body(body).fixtures.clone() {
            let fx = g.world.fixture(f);
            let b = fx.shape.compute_aabb(&g.world.body(fx.body).xf, 0);
            points.push([(b.lower.x + b.upper.x) * 0.5, (b.lower.y + b.upper.y) * 0.5, 0.0]);
        }
        points.into_iter().filter(|&q| !g.overlap_point(q, mask, -0.9, 0.9).is_empty()).collect()
    }

    /// Drags a liquid conduit with the BloodWireTool from `from` to `to`.
    fn lay_wire(g: &mut Game, from: usize, to: usize) {
        g.set_tool_by_type_name("BloodWireTool");
        let mut start = None;
        for q in point_on(g, from) {
            g.input.mouse = q;
            frames(g, 1);
            if g.tool.as_ref().unwrap().hover == Some(from) {
                start = Some(q);
                break;
            }
        }
        assert!(start.is_some(), "no point where the mouse hovers the start object");
        ctl(g, "drag", true, true, false);
        frames(g, 2);
        ctl(g, "drag", false, true, false);
        let mut end = None;
        for q in point_on(g, to) {
            g.input.mouse = q;
            frames(g, 2);
            if g.tool.as_ref().unwrap().hover == Some(to) {
                end = Some(q);
                break;
            }
        }
        assert!(end.is_some(), "no point where the mouse hovers the end object");
        ctl(g, "drag", false, false, true);
        frames(g, 2);
    }

    #[test]
    fn bypass_wire_pumps_a_human_whose_heart_stopped() {
        let Some((mut g, machine)) = spawned("Cardiopulmonary Bypass Machine") else { return };
        let at = g.physicals[machine].transform.position;
        let first_limb = g.limbs.len();
        g.catalog_selected = Some("Human".into());
        g.input.mouse = [at[0] + 3.0, at[1] + 0.5, 0.0];
        ctl(&mut g, "spawnRight", false, false, true);
        frames(&mut g, 1);
        ctl(&mut g, "spawnRight", false, false, false);
        frames(&mut g, 300);
        // The heart stops: the pump limb no longer beats and nothing flows.
        let heart = (first_limb..g.limbs.len()).find(|&l| g.circulations[l].WasInitiallyPumping).expect("a pumping limb");
        for l in first_limb..g.limbs.len() {
            g.circulations[l].IsPump = false;
            g.circulations[l].BloodFlow = 0.0;
        }
        let heart_physical = g.limbs[heart].PhysicalBehaviour;
        lay_wire(&mut g, machine, heart_physical);
        assert_eq!(g.blood_wires.len(), 1);
        assert_eq!(g.undo.history.last().map(|a| a.displayName.as_str()), Some("liquid conduit"));
        let w = &g.blood_wires[0];
        assert!(w.IsCardiopulmonaryBypassWire && !w.IsHeartRateWire && w.started, "{w:?}");
        assert_eq!(g.cardiopulmonary_bypasses[0].ValidConnectionCount, 1);
        // The machine is off: no pumping.
        frames(&mut g, 60);
        assert_eq!(g.circulations[heart].ArtificialHeartbeat, 0.0);
        assert!(g.circulations[heart].BloodFlow < 0.5, "{}", g.circulations[heart].BloodFlow);
        // On: the wire sets the heartbeat and keeps the blood flowing.
        g.broadcast_use(machine, &Act::direct(0));
        assert!(g.cardiopulmonary_bypasses[0].Activated);
        frames(&mut g, 120);
        assert_eq!(g.circulations[heart].ArtificialHeartbeat, g.cardiopulmonary_bypasses[0].TargetHeartrate);
        assert!(g.circulations[heart].BloodFlow > 0.5, "{}", g.circulations[heart].BloodFlow);
        assert!(g.people[g.limbs[heart].Person].OxygenLevel >= 0.5);
        // Undo destroys the wire: OnDestroy stops the heartbeat and drops the connection.
        ctl(&mut g, "undo", false, false, true);
        frames(&mut g, 3);
        assert!(g.destroyed_components.contains(&g.blood_wires[0].id));
        assert!(g.fixed_joints[g.blood_wires[0].joint].destroyed);
        assert_eq!(g.circulations[heart].ArtificialHeartbeat, 0.0);
        assert_eq!(g.cardiopulmonary_bypasses[0].ValidConnectionCount, 0);
        assert!(g.undo.history.iter().all(|a| a.displayName != "liquid conduit" && a.displayName != "LIQUID CONDUIT" || a.RelevantObjects.is_empty()));
    }

    #[test]
    fn defibrillator_restarts_the_circulation_under_its_pads() {
        let Some((mut g, _)) = spawned("Defibrillator") else { return };
        let (pad, layer) = (g.defibrillators[0].Pad1, g.defibrillators[0].PadLayer);
        let at = g.transform_point3(pad, [-0.1, 0.0, 0.0]);
        // A human spawned on the pad.
        let first_limb = g.limbs.len();
        g.catalog_selected = Some("Human".into());
        g.input.mouse = [at[0], at[1], 0.0];
        ctl(&mut g, "spawnRight", false, false, true);
        frames(&mut g, 1);
        let at = g.transform_point3(pad, [-0.1, 0.0, 0.0]);
        for l in first_limb..g.limbs.len() {
            g.circulations[l].IsPump = false;
            g.circulations[l].BloodFlow = 0.0;
            g.people[g.limbs[l].Person].Consciousness = 0.2;
        }
        let charge = g.physicals[pad].Charge();
        g.defibrillate(0, pad, layer);
        assert_eq!(g.physicals[pad].Charge(), charge + 0.2);
        let hit: Vec<usize> = (first_limb..g.limbs.len()).filter(|&l| g.circulations[l].BloodFlow == 1.0).collect();
        assert!(!hit.is_empty(), "no limb under the pad at {at:?}");
        for l in hit {
            assert_eq!(g.circulations[l].IsPump, g.circulations[l].WasInitiallyPumping);
            assert_eq!(g.people[g.limbs[l].Person].Consciousness, 1.0);
            assert!(g.people[g.limbs[l].Person].AdrenalineLevel >= 20.0);
        }
    }

    #[test]
    fn running_chainsaw_saws_a_human_on_its_blade() {
        let Some((mut g, _)) = spawned("Chainsaw") else { return };
        let saw = g.chainsaws[0].physical;
        let at = g.transform_point3(saw, [0.2, 0.0, 0.0]);
        let first_limb = g.limbs.len();
        g.catalog_selected = Some("Human".into());
        g.input.mouse = [at[0], at[1], 0.0];
        ctl(&mut g, "spawnRight", false, false, true);
        frames(&mut g, 1);
        assert!(g.limbs.len() > first_limb, "no human spawned");
        let health: f32 = (first_limb..g.limbs.len()).map(|l| g.limbs[l].Health).sum();
        let (mut connected, mut joints) = (false, 0);
        for _ in 0..120 {
            g.chainsaws[0].rotationSpeed = 10.0;
            frames(&mut g, 1);
            connected |= !g.chainsaws[0].softConnections.keys().is_empty();
            joints = joints.max(g.fixed_joints.iter().filter(|f| f.friction.is_some() && !f.destroyed).count());
        }
        let after: f32 = (first_limb..g.limbs.len()).map(|l| g.limbs[l].Health).sum();
        assert!(connected && joints > 0, "the blade never soft-connected a limb");
        assert!(after < health, "no limb took damage ({health} -> {after})");
    }

    #[test]
    fn emp_generator_zeroes_an_android_limb() {
        let Some((mut g, _)) = spawned("EMP Generator") else { return };
        let emp = g.emps[0].physical;
        let at = g.physicals[emp].transform.position;
        let first_limb = g.limbs.len();
        g.catalog_selected = Some("Android".into());
        g.input.mouse = [at[0] + 2.0, at[1], 0.0];
        ctl(&mut g, "spawnRight", false, false, true);
        frames(&mut g, 1);
        assert!(g.limbs.len() > first_limb, "no android spawned");
        frames(&mut g, 120);
        let androids: Vec<usize> = (first_limb..g.limbs.len()).filter(|&l| g.limbs[l].IsAndroid).collect();
        assert!(!androids.is_empty(), "the spawned Android has no android limbs");
        assert!(androids.iter().all(|&l| g.limbs[l].Health > 0.0), "an android limb starts with no health");
        g.activate_direct(emp);
        frames(&mut g, 1);
        assert!(androids.iter().any(|&l| g.limbs[l].Health == 0.0), "the EMP hit no android limb");
    }

    #[test]
    fn atom_bomb_heats_a_human_and_leaves() {
        let Some((mut g, bomb)) = spawned("Atom Bomb") else { return };
        let at = g.physicals[bomb].transform.position;
        let first_limb = g.limbs.len();
        spawn_at(&mut g, "Human", [at[0] + 6.0, at[1] + 1.0, 0.0]);
        frames(&mut g, 30);
        let limbs: Vec<usize> = (first_limb..g.limbs.len()).map(|l| g.limbs[l].PhysicalBehaviour).collect();
        assert!(limbs.iter().all(|&q| g.physicals[q].Temperature < 500.0), "the human starts hot");
        g.activate_direct(bomb);
        frames(&mut g, 2);
        assert_eq!(g.atom_explosions.len(), 1, "Use instantiated no explosion");
        assert!(g.physicals[bomb].destroyed, "the bomb outlived its Use");
        // Limbs absorb lasers: the ones in line of sight are heated to at least 500, the ones behind them are shaded.
        assert!(limbs.iter().any(|&q| g.physicals[q].Temperature >= 500.0), "no limb was heated");
        frames(&mut g, 30);
        assert!(g.atom_explosions[0].time > 0.0, "the shockwave never grew");
    }

    #[test]
    fn worm_staff_cuts_a_human() {
        let Some((mut g, _)) = spawned("Worm Staff") else { return };
        let staff = g.worm_staffs[0].physical;
        let at = g.physicals[staff].transform.position;
        let first_limb = g.limbs.len();
        spawn_at(&mut g, "Human", [at[0], at[1] + 1.0, 0.0]);
        let limbs: Vec<usize> = (first_limb..g.limbs.len()).collect();
        assert_eq!(g.worm_staffs[0].worms.len(), 3, "Start made no worms");
        let mut frame = 0;
        while frame < 1200 && limbs.iter().all(|&l| g.circulations[l].StabWoundCount == 0) {
            frames(&mut g, 1);
            frame += 1;
        }
        assert!(limbs.iter().any(|&l| g.circulations[l].StabWoundCount > 0), "no worm cut the human in 1200 frames");
    }

    #[test]
    fn tesla_coil_bolts_charge_a_human() {
        let Some((mut g, _)) = spawned("Tesla Coil") else { return };
        let coil = g.tesla_coils[0].physical;
        let at = g.physicals[coil].transform.position;
        let first_limb = g.limbs.len();
        spawn_at(&mut g, "Human", [at[0] + 2.0, at[1] + 1.0, 0.0]);
        frames(&mut g, 120);
        let limbs: Vec<usize> = (first_limb..g.limbs.len()).map(|l| g.limbs[l].PhysicalBehaviour).collect();
        assert!(g.tesla_bolts.iter().all(|b| !b.active), "the bolts are on before Use");
        g.tesla_coil_use(0);
        let mut charged = false;
        for _ in 0..60 {
            frames(&mut g, 1);
            charged |= g.tesla_bolts.iter().any(|b| b.target.is_some_and(|q| limbs.contains(&q) && g.physicals[q].charge > 0.0));
        }
        assert!(charged, "no bolt charged a limb of the human");
        g.tesla_coil_use(0);
        assert!(g.tesla_bolts.iter().all(|b| !b.active), "the bolts stay on after the second Use");
    }

    #[test]
    fn ceiling_turret_stays_frozen_in_place() {
        let Some((mut g, _)) = spawned("Ceiling Turret") else { return };
        // The turret's FreezeBehaviour is the last one added, after the Default map's.
        let turret = g.freezes.last().expect("a FreezeBehaviour").physical;
        assert_eq!(g.physicals[turret].rigidbody.bodyType, 2, "the frozen turret's body is not static");
        let at = g.physicals[turret].transform.position;
        frames(&mut g, 120);
        assert_eq!(g.physicals[turret].transform.position, at, "the frozen turret moved");
    }

    #[test]
    fn ceiling_turret_shoots_a_human_below_it() {
        let Some((mut g, _)) = spawned("Ceiling Turret") else { return };
        let firearm = g.firearms[g.ceiling_turrets.last().expect("a CeilingTurretBehaviour").firearm].physical.expect("a firearm physical");
        let at = g.physicals[firearm].transform.position;
        let first_limb = g.limbs.len();
        g.catalog_selected = Some("Human".into());
        g.input.mouse = [at[0], at[1] - 4.0, 0.0];
        ctl(&mut g, "spawnRight", false, false, true);
        frames(&mut g, 1);
        assert!(g.limbs.len() > first_limb, "no human spawned");
        let health = |g: &Game| -> f32 { (first_limb..g.limbs.len()).map(|l| g.limbs[l].Health).sum() };
        let before = health(&g);
        let mut hit = false;
        for _ in 0..400 {
            frames(&mut g, 1);
            if health(&g) < before {
                hit = true;
                break;
            }
        }
        assert!(hit, "the turret never hit the human ({before} health)");
    }

    #[test]
    fn laser_sentry_turret_shoots_a_human_in_front() {
        let Some((mut g, _)) = spawned("Laser sentry turret") else { return };
        let laser = g.laser_turrets.last().expect("a LaserTurretBehaviour").physical;
        let at = g.physicals[laser].transform.position;
        let first_limb = g.limbs.len();
        g.catalog_selected = Some("Human".into());
        // The barrel points along +x: the human stands 8 units in front of it.
        g.input.mouse = [at[0] + 8.0, at[1], 0.0];
        ctl(&mut g, "spawnRight", false, false, true);
        frames(&mut g, 1);
        assert!(g.limbs.len() > first_limb, "no human spawned");
        let health = |g: &Game| -> f32 { (first_limb..g.limbs.len()).map(|l| g.limbs[l].Health).sum() };
        let before = health(&g);
        let mut hit = false;
        for _ in 0..400 {
            frames(&mut g, 1);
            if health(&g) < before {
                hit = true;
                break;
            }
        }
        assert!(hit, "the laser turret never hit the human ({before} health)");
    }

    #[test]
    fn sentry_turret_shoots_a_human_in_front() {
        let Some((mut g, _)) = spawned("Sentry Turret") else { return };
        let gun = g.sentry_guns.last().expect("a SentryTurretGunBehaviour").node;
        let at = g.node_pose(gun).0;
        let first_limb = g.limbs.len();
        g.catalog_selected = Some("Human".into());
        // The gun's barrel points out of its node: the human stands 8 units in front of it.
        g.input.mouse = [at[0] + 8.0, at[1], 0.0];
        ctl(&mut g, "spawnRight", false, false, true);
        frames(&mut g, 1);
        assert!(g.limbs.len() > first_limb, "no human spawned");
        let health = |g: &Game| -> f32 { (first_limb..g.limbs.len()).map(|l| g.limbs[l].Health).sum() };
        let before = health(&g);
        let mut hit = false;
        for _ in 0..400 {
            frames(&mut g, 1);
            if health(&g) < before {
                hit = true;
                break;
            }
        }
        assert!(hit, "the sentry turret never hit the human ({before} health)");
    }

    #[test]
    fn sentry_turret_explodes_into_debris() {
        let Some((mut g, _)) = spawned("Sentry Turret") else { return };
        let s = g.sentry_turrets.len() - 1;
        let turret = g.sentry_turrets[s].phys;
        let live = |g: &Game| g.physicals.iter().filter(|p| !p.destroyed && p.in_world).count();
        let before = live(&g);
        g.sentry_turret_explode(s);
        frames(&mut g, 2);
        assert!(g.physicals[turret].destroyed || !g.physicals[turret].in_world, "the turret was not destroyed");
        assert!(live(&g) >= before, "the debris did not replace the turret ({before} -> {})", live(&g));
        assert!(g.sentry_turrets[s].exploded, "Explode did not run");
    }

    #[test]
    fn sword_side_blade_soft_connects_a_human() {
        let Some((mut g, _)) = spawned("Sword") else { return };
        let blade = g.sharp_on_all_sides[0].physical;
        let at = g.physicals[blade].transform.position;
        let first_limb = g.limbs.len();
        g.catalog_selected = Some("Human".into());
        g.input.mouse = [at[0], at[1], 0.0];
        ctl(&mut g, "spawnRight", false, false, true);
        frames(&mut g, 1);
        assert!(g.limbs.len() > first_limb, "no human spawned");
        g.sharp_on_all_sides_process(0, true, 10.0);
        let keys = g.sharp_on_all_sides[0].softConnections.keys();
        assert!(!keys.is_empty(), "the blade connected nothing");
        for k in keys {
            let f = g.sharp_on_all_sides[0].softConnections.get(k).unwrap().joint;
            assert_eq!(g.fixed_joints[f].friction, Some((10.0, 10.0)));
        }
        frames(&mut g, 5);
    }

    #[test]
    fn decimator_deletes_a_human_in_its_trigger() {
        let Some((mut g, _)) = spawned("Decimator") else { return };
        let (id, key) = (g.decimators[0].id, g.decimators[0].DecimateTrigger);
        let fixtures = |g: &mut Game| g.with_component_instance(id, |_, inst, _| inst.fixtures.get(&key).map_or(0, |f| f.len()));
        assert_eq!(fixtures(&mut g), 0, "Start leaves the trigger disabled");
        g.decimator_use(0);
        assert!(fixtures(&mut g) > 0, "Use enables the trigger");
        let at = g.with_component_instance(id, |_, inst, _| inst.position(key.0));
        let first_limb = g.limbs.len();
        g.catalog_selected = Some("Human".into());
        g.input.mouse = [at[0], at[1], 0.0];
        ctl(&mut g, "spawnRight", false, false, true);
        frames(&mut g, 1);
        assert!(g.limbs.len() > first_limb, "no human spawned");
        frames(&mut g, 10);
        let head = g.limbs[first_limb].PhysicalBehaviour;
        assert!(g.physicals[head].destroyed, "the human was not deleted");
        g.decimator_use(0);
        assert_eq!(fixtures(&mut g), 0, "Use again disables the trigger");
        frames(&mut g, 5);
    }

    #[test]
    fn immobility_field_holds_a_human_until_switched_off() {
        let Some((mut g, _)) = spawned("Immobility Field") else { return };
        let (id, key, p) = (g.immobility_fields[0].id, g.immobility_fields[0].Trigger, g.immobility_fields[0].physical);
        g.immobility_field_use(0);
        // The field lies beside the device: spawn between its near and far edges.
        let tid = g.with_component_instance(id, |_, inst, _| inst.objects[key.0].components[key.1]);
        let o = g.with_component_instance(id, |_, inst, _| inst.position(key.0));
        let near = g.world.collider_closest_point(tid, V::new(o[0], o[1]));
        let far = g.world.collider_closest_point(tid, V::new(2.0 * near.x - o[0] + 20.0 * (near.x - o[0]), near.y));
        let at = [(near.x + far.x) / 2.0, near.y, 0.0];
        let first_limb = g.limbs.len();
        g.catalog_selected = Some("Human".into());
        g.input.mouse = [at[0], at[1], 0.0];
        ctl(&mut g, "spawnRight", false, false, true);
        frames(&mut g, 1);
        assert!(g.limbs.len() > first_limb, "no human spawned");
        frames(&mut g, 10);
        let held = |g: &Game| g.fixed_joints.iter().filter(|f| f.friction.is_some() && !f.destroyed && matches!(f.attached, BodyOwner::Physical(a) if a == p)).count();
        assert!(!g.immobility_fields[0].joints.keys().is_empty() && held(&g) > 0, "the field caught nothing");
        g.immobility_field_use(0);
        assert!(g.immobility_fields[0].joints.keys().is_empty(), "switching off keeps the captures");
        frames(&mut g, 2);
        assert_eq!(held(&g), 0, "switching off keeps the joints");
    }

    #[test]
    fn energy_sword_cuts_a_human_and_switches_off() {
        let Some((mut g, _)) = spawned("Energy Sword") else { return };
        let sword = g.energy_swords[0].physical;
        g.energy_sword_use(0);
        frames(&mut g, 1);
        assert!(g.with_component_instance(g.energy_swords[0].id, |g, inst, _| inst.objects[g.energy_swords[0].BladeObject].active), "the blade did not switch on");
        let line = g.energy_swords[0].LocalBladeLine;
        let mid = [(line[0][0] + line[1][0]) / 2.0, (line[0][1] + line[1][1]) / 2.0];
        let at = g.transform_point3(sword, [mid[0], mid[1], 0.0]);
        let first_limb = g.limbs.len();
        g.catalog_selected = Some("Human".into());
        g.input.mouse = [at[0], at[1], 0.0];
        ctl(&mut g, "spawnRight", false, false, true);
        frames(&mut g, 1);
        assert!(g.limbs.len() > first_limb, "no human spawned");
        let health: f32 = (first_limb..g.limbs.len()).map(|l| g.limbs[l].Health).sum();
        let mut connected = false;
        for _ in 0..60 {
            frames(&mut g, 1);
            connected |= !g.energy_swords[0].softConnections.keys().is_empty();
        }
        let after: f32 = (first_limb..g.limbs.len()).map(|l| g.limbs[l].Health).sum();
        assert!(connected, "the blade never connected a limb");
        assert!(after < health, "no limb took damage ({health} -> {after})");
        g.energy_sword_use(0);
        frames(&mut g, 2);
        assert!(g.energy_swords[0].softConnections.keys().is_empty(), "switching off left connections");
        let id = g.energy_swords[0].id;
        g.destroy_object(id);
        frames(&mut g, 2);
    }

    /// Spawns `item` at `at` and returns its first physical.
    fn spawn_at(g: &mut Game, item: &str, at: [f32; 3]) -> usize {
        let first = g.physicals.len();
        g.catalog_selected = Some(item.into());
        g.input.mouse = at;
        ctl(g, "spawnRight", false, false, true);
        frames(g, 1);
        ctl(g, "spawnRight", false, false, false);
        assert!(g.physicals.len() > first, "{item}: nothing spawned");
        first
    }

    #[test]
    fn syringe_pushes_into_and_pulls_from_a_flask() {
        use crate::game::container::Owner;
        let Some((mut g, flask_p)) = spawned("Flask") else { return };
        let f = g.flasks.iter().position(|x| x.Phys == Some(flask_p)).expect("the flask");
        let at = g.physicals[flask_p].transform.position;
        let life = crate::game::circulation::liquid("LIFE SERUM");
        // A finite Life Syringe lodged in the flask pushes its serum in (SyringeBehaviour.push, PressureMode Push).
        let s = g.syringes.len();
        spawn_at(&mut g, "Life Syringe", [at[0] + 1.0, at[1] + 1.0, 0.0]);
        let life_syringe = g.syringes.len() - 1;
        assert_eq!(life_syringe, s);
        g.syringes[s].Finite = true;
        g.syringe_lodged(s, flask_p);
        assert_eq!(g.syringes[s].pushTargets, [Some(Owner::Flask(f))]);
        let before = g.bc(Owner::Syringe(s)).TotalLiquidAmount;
        frames(&mut g, 120);
        let in_flask = g.get_amount(Owner::Flask(f), life);
        assert!(in_flask > 0.1, "nothing reached the flask ({in_flask})");
        assert!(g.bc(Owner::Syringe(s)).TotalLiquidAmount < before, "the syringe kept its liquid");
        // An empty finite syringe on Pull draws it back out (pull: other.TransferTo(TransferRate, this)).
        let e = g.syringes.len();
        spawn_at(&mut g, "Empty Syringe", [at[0] - 1.0, at[1] + 1.0, 0.0]);
        g.syringes[e].Finite = true;
        g.syringe_use(e, 2);
        g.syringe_lodged(e, flask_p);
        assert_eq!(g.bc(Owner::Syringe(e)).TotalLiquidAmount, 0.0);
        frames(&mut g, 120);
        assert!(g.get_amount(Owner::Syringe(e), life) > 0.01, "the empty syringe drew nothing");
    }

    #[test]
    fn physics_gun_holds_a_human_limb_and_lets_go() {
        let Some((mut g, _)) = spawned("Human") else { return };
        // The leftmost limb: the gun's ray, aimed right at its height, meets the human there.
        let centre = |g: &Game, p: usize| g.world.body(g.physicals[p].rigidbody.body).sweep.c;
        let human: Vec<usize> = g.limbs.iter().map(|l| l.PhysicalBehaviour).collect();
        let left = human.iter().copied().min_by(|&a, &b| centre(&g, a).x.partial_cmp(&centre(&g, b).x).unwrap()).unwrap();
        let c = centre(&g, left);
        spawn_at(&mut g, "Physics Gun", [c.x - 1.2, c.y - 0.04, 0.0]);
        let gun = g.physics_guns.len() - 1;
        let gun_p = g.physics_guns[gun].physical;
        // Held down (Value > 0 each frame) it grabs the first physical its ray meets.
        for _ in 0..30 {
            g.physicals[gun_p].ContinuousActivationTracker[0].Value = 1;
            frames(&mut g, 1);
            if g.physics_guns[gun].Target.is_some() {
                break;
            }
        }
        let target = g.physics_guns[gun].Target.expect("the gun grabbed nothing");
        assert!(human.contains(&target), "the gun grabbed something else than the human");
        let f = g.physics_guns[gun].joint.expect("no RelativeJoint2D");
        let j = &g.fixed_joints[f];
        let r = j.relative.expect("not a RelativeJoint2D");
        assert!(j.joint.is_some() && !j.destroyed && j.state.enable_collision, "the joint is not live");
        assert!(matches!(j.connected, Some(BodyOwner::Physical(t)) if t == target) && matches!(j.attached, BodyOwner::Physical(a) if a == gun_p));
        assert_eq!((r.max_force, r.max_torque), (1200.0, 300.0));
        // It carries the limb along when the gun rises: the limb follows (the gun swings on it) at the same distance.
        let before = centre(&g, target).y;
        let held_distance = |g: &Game| {
            let (a, b) = (centre(g, gun_p), centre(g, target));
            ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
        };
        let start = held_distance(&g);
        for _ in 0..60 {
            g.physicals[gun_p].ContinuousActivationTracker[0].Value = 1;
            g.physicals[gun_p].rigidbody.set_velocity(&mut g.world, [0.0, 3.0]);
            frames(&mut g, 1);
        }
        let after = centre(&g, target).y;
        assert!(after > before + 0.1, "the held limb did not follow the gun ({before} -> {after})");
        assert!((held_distance(&g) - start).abs() < 0.05, "the joint let the limb drift from the gun ({start} -> {})", held_distance(&g));
        // Letting go destroys the joint.
        g.physicals[gun_p].ContinuousActivationTracker[0].Value = 0;
        frames(&mut g, 4);
        assert!(g.physics_guns[gun].Target.is_none(), "the gun still has a target");
        assert!(g.fixed_joints[f].destroyed && g.fixed_joints[f].joint.is_none(), "the joint stayed");
    }

    #[test]
    fn activator_electrode_activates_the_hand_it_touches() {
        let Some((mut g, _)) = spawned("Human") else { return };
        let grip = 1;
        let hand = g.grips[grip].PhysicalBehaviour;
        // The hand's grip point: the electrode's emission point goes there, so the hand is touched and can reach the electrode.
        let c = g.transform_point3(hand, g.grips[grip].GripPosition);
        // Something the grip can hold, within its reach but out of the electrode's emission circle.
        let pistol = spawn_at(&mut g, "Pistol", [c[0], c[1] - 0.35, 0.0]);
        assert!(!g.physicals[pistol].HoldingPositions.is_empty());
        spawn_at(&mut g, "Activator Electrode", [c[0] - 0.39, c[1], 0.0]);
        let e = g.activator_electrodes.len() - 1;
        let ep = g.activator_electrodes[e].physical;
        assert!(!g.grips[grip].isHolding);
        // Using the electrode passes the Use on to the object at its emission point: the hand's GripBehaviour picks up.
        g.activate_direct(ep);
        assert!(g.grips[grip].isHolding && g.grips[grip].CurrentlyHolding == Some(pistol), "the touched hand did not take the pistol");
        // A continuous use reaches the first physical at the emission point on the same channel.
        let before: i32 = (0..g.physicals.len()).filter(|&p| p != ep).map(|p| g.physicals[p].ContinuousActivationTracker[0].Value).sum();
        g.broadcast_use_continuous(ep, &Act::direct(0));
        let after: i32 = (0..g.physicals.len()).filter(|&p| p != ep).map(|p| g.physicals[p].ContinuousActivationTracker[0].Value).sum();
        assert!(after > before, "the continuous use reached nothing");
        // A disabled electrode passes nothing on.
        g.activator_electrodes[e].enabled = false;
        g.broadcast_use_continuous(ep, &Act::direct(0));
        let again: i32 = (0..g.physicals.len()).filter(|&p| p != ep).map(|p| g.physicals[p].ContinuousActivationTracker[0].Value).sum();
        assert_eq!(again, after);
    }

    /// Spawns `item` facing right, 1.2 to the left of the human's leftmost limb at its height, and holds its
    /// continuous use for `n` frames. Returns the item's physical and the human's limb physicals.
    fn spray_at_human(g: &mut Game, item: &str, n: usize, mut each: impl FnMut(&mut Game)) -> (usize, Vec<usize>) {
        let centre = |g: &Game, p: usize| g.world.body(g.physicals[p].rigidbody.body).sweep.c;
        let human: Vec<usize> = g.limbs.iter().map(|l| l.PhysicalBehaviour).collect();
        let left = human.iter().copied().min_by(|&a, &b| centre(g, a).x.partial_cmp(&centre(g, b).x).unwrap()).unwrap();
        let c = centre(g, left);
        let first = spawn_at(g, item, [c.x - 1.2, c.y, 0.0]);
        for _ in 0..n {
            g.physicals[first].ContinuousActivationTracker[0].Value = 1;
            each(g);
            frames(g, 1);
        }
        (first, human)
    }

    #[test]
    fn flamethrower_heats_and_ignites_the_human_it_sprays() {
        let Some((mut g, _)) = spawned("Human") else { return };
        let human: Vec<usize> = g.limbs.iter().map(|l| l.PhysicalBehaviour).collect();
        let before: Vec<f32> = human.iter().map(|&p| g.physicals[p].Temperature).collect();
        assert!(human.iter().all(|&p| !g.physicals[p].OnFire));
        let mut lit = false;
        spray_at_human(&mut g, "Flamethrower", 120, |g| lit |= g.limbs.iter().any(|l| g.physicals[l.PhysicalBehaviour].OnFire));
        lit |= human.iter().any(|&p| g.physicals[p].OnFire);
        let heated = human.iter().zip(&before).any(|(&p, &t)| g.physicals[p].Temperature > t + 0.5);
        assert!(lit || heated, "the flames never reached the human");
    }

    #[test]
    fn fire_extinguisher_puts_out_the_burning_limb_it_sprays() {
        let Some((mut g, _)) = spawned("Human") else { return };
        let centre = |g: &Game, p: usize| g.world.body(g.physicals[p].rigidbody.body).sweep.c;
        let human: Vec<usize> = g.limbs.iter().map(|l| l.PhysicalBehaviour).collect();
        let left = human.iter().copied().min_by(|&a, &b| centre(&g, a).x.partial_cmp(&centre(&g, b).x).unwrap()).unwrap();
        let right = human.iter().copied().max_by(|&a, &b| centre(&g, a).x.partial_cmp(&centre(&g, b).x).unwrap()).unwrap();
        for &p in &[left, right] {
            g.physicals[p].OnFire = true;
            g.physicals[p].burnIntensity = 1.0;
        }
        spray_at_human(&mut g, "Fire Extinguisher", 60, |_| {});
        assert!(!g.physicals[left].OnFire && g.physicals[left].burnIntensity == 0.0, "the sprayed limb is still burning");
        assert!(g.physicals[right].OnFire, "the limb out of reach was put out too");
    }
}
