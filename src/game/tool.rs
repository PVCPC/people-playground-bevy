//! ToolControllerBehaviour with its SelectionController and DragTool (the tool the game starts with), fed by
//! InputSystem's controls and Global.MousePosition.
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

/// b2NearestPointOnLineSegment (180bb1780).
fn nearest_point_on_segment(p: V, a: V, b: V) -> V {
    let (ex, ey) = (b.x - a.x, b.y - a.y);
    let len2 = ex * ex + ey * ey;
    if f32::from_bits(0x28800000) > len2 {
        return a;
    }
    let t = (ex * (p.x - a.x) + ey * (p.y - a.y)) / len2;
    let t = if t < 1.0 { t } else { 1.0 };
    let t = if 0.0 > t { 0.0 } else { t };
    V::new(a.x + ex * t, a.y + ey * t)
}

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
        hits
    }

    /// Physics2D.OverlapPoint(point, mask): the lowest-z collider at the point.
    pub fn overlap_point_first(&mut self, point: [f32; 2], mask: u32) -> Option<usize> {
        self.sync_pending();
        let p = V::new(point[0], point[1]);
        self.overlap_query(self.world.query_point(p), mask, f32::NEG_INFINITY, f32::INFINITY, |shape, _, xf| test_point(shape, xf, p)).first().copied()
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
        self.overlap_query(self.world.query_aabb(&aabb), mask, f32::NEG_INFINITY, f32::INFINITY, |other, child, xf| shape.test_overlap(0, &identity, other, child, xf))
    }

    /// The overlap queries' shared filter: sensors skipped (QueriesHitTriggers off), the layer mask, the depth
    /// range on the Transform's z, each collider once, then a stable sort by z. The callers run PhysicsQuery2D's
    /// AutoSyncTransforms (Physics2DSettings.autoSyncTransforms is on) before querying.
    fn overlap_query(&self, fixtures: Vec<(crate::b2::world::FixtureId, usize)>, mask: u32, min_depth: f32, max_depth: f32, test: impl Fn(&Shape, usize, &crate::b2::math::Transform) -> bool) -> Vec<usize> {
        let mut hits: Vec<(usize, i32)> = Vec::new();
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
            let Some(ph) = self.physical_of_collider(fx.collider.id) else {
                unimplemented!("OverlapPoint: a selectable collider without a PhysicalBehaviour");
            };
            let z = self.physicals[ph].transform.position[2];
            if z < min_depth || max_depth < z {
                continue;
            }
            if hits.iter().any(|&(_, c)| c == fx.collider.id) {
                continue;
            }
            if test(&fx.shape, child, &self.world.body(fx.body).xf) {
                hits.push((ph, fx.collider.id));
            }
        }
        hits.sort_by(|a, b| self.physicals[a.0].transform.position[2].partial_cmp(&self.physicals[b.0].transform.position[2]).unwrap());
        hits.into_iter().map(|(ph, _)| ph).collect()
    }


    /// The physical's Transform ancestors, root first, then its own local TRS.
    pub(super) fn local_chain(&self, p: usize) -> Vec<crate::unity::transform::Trs> {
        use crate::unity::transform::{inverse, Trs};
        let mut chain = Vec::new();
        let mut n = self.physicals[p].parent;
        while let Some(i) = n {
            chain.push(self.transform_nodes[i].trs);
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
        if self.tool.is_some() && self.input.held("drag") {
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
        if self.input.down("activateDirect") && self.tool.as_ref().unwrap().ActiveSingleSelected.is_none() && self.tool.as_ref().unwrap().hover.is_some() {
            unimplemented!("activateDirect: Utils.Activations.SendOnce / Use");
        }
        self.determine_hovering();
        self.handle_tools();
        // HandleContextMenu (ContextMenuBehaviour.Show is UI); UpdateSelectionBox draws the box.
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
                        self.drag_on_deselect();
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
                        self.drag_on_deselect();
                    }
                    self.tool.as_mut().unwrap().ActiveSingleSelected = Some(h);
                }
                None => {
                    self.escape_and_deselect();
                    // DragTool.UsesEmptyDrag is false: StartSelectionDrag.
                    let t = self.tool.as_mut().unwrap();
                    if !t.isSelecting {
                        let Some(screen) = self.input.screen else { unimplemented!("StartSelectionDrag: Input.mousePosition in a recording") };
                        t.isSelecting = true;
                        t.selectionDragStart = screen.mouse;
                    }
                }
            }
            self.tool.as_mut().unwrap().isCurrentlyHolding = true;
            self.drag_on_select();
        }
        if self.input.down("context") && self.tool.as_ref().unwrap().currentlyHovering.is_some() {
            let t = self.tool.as_ref().unwrap();
            let h = t.currentlyHovering.unwrap();
            let multiple = fast || t.rawSelection.contains(&h);
            self.select(h, multiple);
        }
        if self.input.held("drag") && self.tool.as_ref().unwrap().isCurrentlyHolding {
            self.drag_on_hold();
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
        self.drag_on_deselect();
        let fast = self.input.held("fast");
        let t = self.tool.as_mut().unwrap();
        t.ActiveSingleSelected = None;
        t.isCurrentlyHolding = false;
        let kept = t.currentlyHovering.is_some_and(|h| t.rawSelection.contains(&h));
        if !kept && !fast {
            t.rawSelection.clear();
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
            unimplemented!("activateDirect on the selection: Utils.Activations.SendOnce / Use");
        }
        self.tool.as_mut().unwrap().shouldSendContinuousUseMessage = self.input.held("activateDirect");
        if self.tool.as_ref().unwrap().shouldSendContinuousUseMessage {
            unimplemented!("ContinuousActivationBehaviour: UseContinuous");
        }
        if self.input.held("delete") {
            unimplemented!("SelectionController.UserDeleteSelection");
        }
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
        if t.ActiveSingleSelected.is_none() || t.selectedRigidbody.is_none() {
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
