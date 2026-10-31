//! BloodWireBehaviour with the WireBehaviour, Hover and DistanceJointWireBehaviour it derives from, and the BloodWireTool
//! (DistanceWireTool) that lays it.
//!
//! The tool drags a wire from the selected object to the object under the mouse: OnDeselect adds a DistanceJoint2D
//! (maxDistanceOnly, enableCollision) between them and a BloodWireBehaviour, registered in the undo history as
//! "liquid conduit". The wire's FixedUpdate moves liquid from the higher-pressure container to the other and averages
//! the pressures; a wire between a CardiopulmonaryBypassMachineBehaviour and a living limb sets the limb's
//! ArtificialHeartbeat and BloodFlow every frame, and one to a HeartMonitorBehaviour feeds it the connected limb.
//!
//! Presentation, skipped: the LineRenderers (the tool's and the wire's: positions, colours, width, material, flow and
//! blood-colour property block, the stress colour that needs ShowLimbStatus), the machine's sounds and status text (and
//! the 3 s `flash` coroutine that only writes text), the Wire child's GameObject (4 instance ids are taken), Stats and
//! the modding events. DistanceJointWireBehaviour.Start of a wire without a typedJoint (a saved wire) and OnFlip /
//! OnPreFlip (SelectionController.FlipSelection) are not reachable. Hover.CurrentlyHovering is only read by the context
//! menu.
#![allow(non_snake_case)]

use super::container::Owner;
use super::undo::ObjectCreationAction;
use super::{mathf, BodyOwner, Behaviour, Game};

/// One BloodWireBehaviour added by the tool.
#[derive(Clone, Debug, Default)]
pub struct BloodWire {
    /// The component's instance id.
    pub id: i32,
    /// physicalBehaviour: the PhysicalBehaviour on its GameObject (GetComponent in Awake).
    pub physical: usize,
    /// otherPhysicalBehaviour: that of the joint's connectedBody, set in Start.
    pub other: Option<usize>,
    /// typedJoint / untypedJoint: the `Game::fixed_joints` entry of the DistanceJoint2D.
    pub joint: usize,
    /// WireWidth (the tool's 0.05).
    pub WireWidth: f32,
    /// Hover.Invisible
    pub Invisible: bool,
    /// Start ran (Update and FixedUpdate wait for it).
    pub started: bool,
    pub usedToHaveConnectedBody: bool,
    pub shouldIgnoreUpdate: bool,
    pub IsMouseInsideBounds: bool,
    pub IsMouseOverlapping: bool,
    /// The wire's vertices (13); the LineRenderer holds the same positions.
    pub vertices: [[f32; 3]; 13],
    pub limbA: Option<usize>,
    pub limbB: Option<usize>,
    pub containerA: Option<Owner>,
    pub containerB: Option<Owner>,
    pub heartMonitor: Option<usize>,
    pub cardiopulmonaryBypass: Option<usize>,
    pub heartLimb: Option<usize>,
    pub AreBothLimbs: bool,
    pub IsHeartRateWire: bool,
    pub IsCardiopulmonaryBypassWire: bool,
}

/// The BloodWireTool (a DistanceWireTool) on the ToolController: whether it is the CurrentTool, and its drag.
#[derive(Clone, Debug, Default)]
pub struct WireTool {
    /// CurrentTool is the BloodWireTool (else the DragTool).
    pub chosen: bool,
    /// The BloodWireTool component and the LineRenderer OnToolChosen adds exist.
    pub created: bool,
    pub startPos: [f32; 2],
    pub startOffset: [f32; 2],
    pub hasDragged: bool,
}

/// Utils.SqrdDistanceFromPointToLineSegment (a degenerate segment returns the distance itself).
fn sqrd_distance_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let dot = |u: [f32; 2], v: [f32; 2]| (super::d(u[0]) * v[0] + super::d(u[1]) * v[1]).f();
    let sqr = mathf::sqr_magnitude([a[0] - b[0], a[1] - b[1]]);
    if sqr as f64 == 0.0 {
        return mathf::magnitude2([p[0] - a[0], p[1] - a[1]]);
    }
    let num = mathf::clamp01(dot([p[0] - a[0], p[1] - a[1]], [b[0] - a[0], b[1] - a[1]]) / sqr);
    let v = [a[0] + num * (b[0] - a[0]), a[1] + num * (b[1] - a[1])];
    mathf::sqr_magnitude([p[0] - v[0], p[1] - v[1]])
}

/// Utils.Cosh
fn cosh(x: f32) -> f32 {
    (mathf::pow(std::f64::consts::E as f32, x) + mathf::pow(std::f64::consts::E as f32, 0.0 - x)) / 2.0
}

impl Game {
    /// WireSnapController.GetTransformedWorldEndPoint
    fn wire_end_point(&self, end: [f32; 2], start: [f32; 2]) -> [f32; 2] {
        if !self.input.held("snap") {
            return end;
        }
        let v = [end[0] - start[0], end[1] - start[1]];
        let magnitude = mathf::magnitude2(v);
        if magnitude < 0.001 {
            return end;
        }
        let dir = [v[0] / magnitude, v[1] / magnitude];
        // Utils.Snap(Mathf.Atan2(y, x), (float)Math.PI / 4f): Mathf.Round is Math.Round, to even.
        let atan = (dir[1] as f64).atan2(dir[0] as f64) as f32;
        let step = std::f64::consts::PI as f32 / 4.0;
        let f = if mathf::approximately(step, 0.0) { atan } else { (atan / step).round_ties_even() * step };
        [(super::d(start[0]) + super::d(mathf::cos(f)) * magnitude).f(), (super::d(start[1]) + super::d(mathf::sin(f)) * magnitude).f()]
    }

    /// `(bool)ActiveSingleSelected`: the tool's selected object, unless it was destroyed.
    fn wire_selected(&self) -> Option<usize> {
        self.tool.as_ref().and_then(|t| t.ActiveSingleSelected).filter(|&p| !self.physicals[p].destroyed)
    }

    /// DistanceWireTool.OnSelect (the LineRenderer is presentation).
    pub(super) fn wire_on_select(&mut self) {
        let m = self.MousePosition;
        self.wire_tool.startPos = [m[0], m[1]];
        if let Some(sel) = self.wire_selected() {
            let o = self.inverse_transform_point(sel, [m[0], m[1], 0.0]);
            self.wire_tool.startOffset = [o[0], o[1]];
        }
        self.wire_tool.hasDragged = true;
    }

    /// DistanceWireTool.OnHold
    pub(super) fn wire_on_hold(&mut self) {
        if let Some(sel) = self.wire_selected() {
            let o = self.wire_tool.startOffset;
            let p = self.transform_point3(sel, [o[0], o[1], 0.0]);
            self.wire_tool.startPos = [p[0], p[1]];
        }
    }

    /// The layer of the PhysicalBehaviour's GameObject (that of its Rigidbody2D).
    fn physical_game_object_layer(&self, p: usize) -> i32 {
        let go = self.world.body(self.physicals[p].rigidbody.body).user.expect("a physical's Rigidbody2D").game_object;
        self.world.layers[&go]
    }

    /// DistanceWireTool.OnDeselect of the BloodWireTool (AllowNothingConnection is false, so a wire needs both ends).
    pub(super) fn wire_on_deselect(&mut self) {
        if !self.wire_tool.hasDragged {
            return;
        }
        // GetEndObject: SelectionController.Main.CurrentlyUnderMouse.
        let m = self.MousePosition;
        let start = self.wire_tool.startPos;
        let end = self.wire_end_point([m[0], m[1]], start);
        let under = self.tool.as_ref().and_then(|t| t.hover).filter(|&p| !self.physicals[p].destroyed);
        let end_body = under.filter(|&p| self.physical_game_object_layer(p) != 11);
        if let (Some(sel), Some(eb)) = (self.wire_selected(), end_body) {
            self.allocate_instance_id();
            let f = self.add_distance_joint_component(sel);
            let anchor = self.inverse_transform_point(sel, [start[0], start[1], 0.0]);
            self.fixed_joint_set_anchor(f, [anchor[0], anchor[1]]);
            self.fixed_joint_set_auto_configure(f, false);
            self.fixed_joint_set_connected_body(f, Some(BodyOwner::Physical(eb)));
            let connected = self.inverse_transform_point(eb, [end[0], end[1], 0.0]);
            self.fixed_joint_set_connected_anchor(f, [connected[0], connected[1]]);
            self.fixed_joint_set_enable_collision(f, true);
            self.distance_joint_set_max_distance_only(f, true);
            // NonSteamStatManager.Stats.Increment("WIRES_LAID") is a statistic.
            self.blood_wire_tool_on_joint_create(sel, f);
        }
        self.wire_tool.hasDragged = false;
    }

    /// BloodWireTool.OnJointCreate: AddComponent<BloodWireBehaviour>() (Awake: GetComponent<PhysicalBehaviour>), the undo
    /// action, WireWidth 0.05 and Invisible = (ToolVisibility == Invisible), which is never set.
    fn blood_wire_tool_on_joint_create(&mut self, sel: usize, f: usize) {
        let id = self.allocate_instance_id();
        self.blood_wires.push(BloodWire { id, physical: sel, joint: f, WireWidth: 0.05, ..Default::default() });
        let w = self.blood_wires.len() - 1;
        let b = Behaviour::BloodWire(w);
        // BloodWireBehaviour's MonoScript execution order is 0.
        self.behaviour_of.insert(id, (0, b));
        self.behaviours.add(0, b);
        self.pending_starts.push(super::misc::PendingStart::BloodWire(w));
        self.undo_register_action(ObjectCreationAction { displayName: "liquid conduit".into(), RelevantObjects: vec![id] });
    }

    /// ToolControllerBehaviour.SetTool's BloodWireTool half: AddComponent the first time (Awake loads the material), then
    /// OnToolChosen (GetComponent<LineRenderer> or AddComponent, and its properties).
    pub(super) fn wire_tool_choose(&mut self) {
        if !self.wire_tool.created {
            self.allocate_instance_id();
            self.allocate_instance_id();
            self.wire_tool.created = true;
        }
        self.wire_tool.chosen = true;
    }

    /// `(bool)joint.connectedBody`: set, and its Rigidbody2D not destroyed.
    fn wire_connected_alive(&self, f: usize) -> bool {
        match self.fixed_joints[f].connected {
            None => false,
            Some(BodyOwner::Physical(p)) => !self.destroyed_components.contains(&self.physicals[p].rigidbody.component),
            Some(BodyOwner::Node(k)) => !self.destroyed_components.contains(&self.node_rigidbodies[k].rigidbody.component),
        }
    }

    /// `(bool)physical`: the component was not destroyed.
    fn wire_physical_alive(&self, p: usize) -> bool {
        !self.physicals[p].destroyed
    }

    /// physical.GetComponent<BloodContainer>(): a limb, flask, syringe, blood tank or valve.
    fn wire_container(&mut self, q: usize) -> Option<Owner> {
        if let Some(v) = self.valves.iter().position(|x| x.physical == q) {
            return Some(Owner::Valve(v));
        }
        self.physical_container(q)
    }

    /// `(bool)container`: its GameObject's PhysicalBehaviour was not destroyed.
    fn wire_container_alive(&self, o: Owner) -> bool {
        self.wire_physical_alive(self.container_physical(o))
    }

    /// AllowsTransfer and AllowPressureTransfer: a ValveBehaviour's is ValvePosition > 0.5 (both), every other container
    /// rewritten is true.
    fn wire_allows(&self, o: Owner) -> bool {
        match o {
            Owner::Valve(v) => self.valves[v].ValvePosition > 0.5,
            _ => true,
        }
    }

    /// physical.GetComponent<CirculationBehaviour>(): a limb.
    fn wire_limb(&self, q: usize) -> Option<usize> {
        self.limbs.iter().position(|x| x.PhysicalBehaviour == q)
    }

    /// DistanceJointWireBehaviour.Start (with a typedJoint) and WireBehaviour.Initialise: the Wire child object (a
    /// GameObject, its Transform, Optout and the LineRenderer take 4 instance ids), the other end's PhysicalBehaviour,
    /// then Created.
    pub fn blood_wire_start(&mut self, w: usize) {
        if self.destroyed_components.contains(&self.blood_wires[w].id) {
            return;
        }
        for _ in 0..4 {
            self.allocate_instance_id();
        }
        let f = self.blood_wires[w].joint;
        if self.wire_connected_alive(f) {
            self.blood_wires[w].usedToHaveConnectedBody = true;
            self.blood_wires[w].other = match self.fixed_joints[f].connected {
                Some(BodyOwner::Physical(p)) => Some(p),
                _ => unimplemented!("WireBehaviour.Initialise: GetComponent<PhysicalBehaviour>() of a node's Rigidbody2D"),
            };
        }
        self.blood_wire_created(w);
        self.blood_wires[w].started = true;
    }

    /// BloodWireBehaviour.Created
    fn blood_wire_created(&mut self, w: usize) {
        let (p, other) = (self.blood_wires[w].physical, self.blood_wires[w].other);
        let Some(o) = other.filter(|&o| o != p && self.wire_physical_alive(o)) else {
            self.blood_wires[w].AreBothLimbs = false;
            return;
        };
        let container_a = self.wire_container(p);
        let container_b = self.wire_container(o);
        let (limb_a, limb_b) = (self.wire_limb(p), self.wire_limb(o));
        let monitor = self.heart_monitors.iter().position(|x| x.physical == p).or_else(|| self.heart_monitors.iter().position(|x| x.physical == o));
        let cpb = self.cardiopulmonary_bypasses.iter().position(|x| x.physical == p).or_else(|| self.cardiopulmonary_bypasses.iter().position(|x| x.physical == o));
        let wire = &mut self.blood_wires[w];
        wire.containerA = container_a;
        wire.containerB = container_b;
        wire.limbA = limb_a;
        wire.limbB = limb_b;
        wire.heartMonitor = monitor;
        wire.cardiopulmonaryBypass = cpb;
        wire.IsHeartRateWire = monitor.is_some();
        let first = limb_a.or(limb_b);
        let cpb_wire = cpb.is_some()
            && first.is_some_and(|l| self.circulations[l].WasInitiallyPumping && !self.limbs[l].IsAndroid);
        self.blood_wires[w].IsCardiopulmonaryBypassWire = cpb_wire;
        if cpb_wire {
            // PlayValidSound (audio) and UpdateStatusText (text).
            self.cardiopulmonary_bypasses[cpb.unwrap()].ValidConnectionCount += 1;
        }
        // Else with a machine: PlayInvalidSound, and the INORGANIC / ARTIFICIAL / NO HEART DETECTED text that `flash` shows
        // for 3 s: audio and text.
        if self.blood_wires[w].IsHeartRateWire {
            let heart = [limb_a, limb_b].into_iter().flatten().find(|&l| !self.limbs[l].IsAndroid);
            match heart {
                Some(l) => self.blood_wires[w].heartLimb = Some(l),
                None => self.blood_wires[w].IsHeartRateWire = false,
            }
        }
        self.blood_wires[w].AreBothLimbs = matches!((limb_a, limb_b), (Some(a), Some(b)) if !self.limbs[a].IsAndroid && !self.limbs[b].IsAndroid);
    }

    /// Hover.CheckMouseInput of a WireBehaviour (no dialog, UI block or pause menu is ever open): deletes the wire while
    /// "delete" is held over it.
    fn blood_wire_check_mouse_input(&mut self, w: usize) {
        let m = self.MousePosition;
        let v = self.blood_wires[w].vertices;
        // lineRenderer.bounds (LineRenderer::CalculateWorldMatrixAndBoundsJob, a world-space line): centre (max + min) * 0.5
        // and extents (max - min) * 0.5 + widthMultiplier * 0.5 on each axis; IsPointInsideBounds is strict.
        let width = self.blood_wires[w].WireWidth;
        let inside_bounds = {
            let mut inside = true;
            for i in 0..2 {
                let (lo, hi) = v.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[i]), hi.max(p[i])));
                let (centre, extent) = ((hi + lo) * 0.5, (hi - lo) * 0.5 + width * 0.5);
                inside &= m[i] > centre - extent && m[i] < centre + extent;
            }
            inside
        };
        self.blood_wires[w].IsMouseInsideBounds = inside_bounds;
        // IsMouseInsideCollider: the mouse within WireWidth of one of the segments.
        let limit = width * width;
        let on_wire = inside_bounds && (0..12).any(|i| sqrd_distance_to_segment([m[0], m[1]], [v[i][0], v[i][1]], [v[i + 1][0], v[i + 1][1]]) <= limit);
        let flag = on_wire;
        if flag && self.input.held("delete") && !self.blood_wires[w].Invisible {
            // OnUserDelete is empty; Object.Destroy(this).
            let id = self.blood_wires[w].id;
            self.tr_destroy_component(id, 0.0);
        }
        // OnMouseEnter / OnMouseExit have the ToolInvisibilityTracker's outline material (presentation).
        self.blood_wires[w].IsMouseOverlapping = flag;
    }

    /// DistanceJointWireBehaviour.CalculateParabola into `vertices`.
    fn blood_wire_calculate_parabola(&mut self, w: usize) {
        let (p, f) = (self.blood_wires[w].physical, self.blood_wires[w].joint);
        let j = &self.fixed_joints[f];
        let (anchor, connected_anchor) = (j.state.anchor, j.state.connected_anchor);
        let a = self.transform_point3(p, [anchor.x, anchor.y, 0.0]);
        let b = match (self.wire_connected_alive(f), j.connected) {
            (true, Some(BodyOwner::Physical(o))) => self.transform_point3(o, [connected_anchor.x, connected_anchor.y, 0.0]),
            (true, _) => unimplemented!("DistanceJointWireBehaviour.Update: the TransformPoint of a node's Rigidbody2D"),
            (false, _) => [connected_anchor.x, connected_anchor.y, 0.0],
        };
        let distance = self.distance_joint_distance(f);
        let count = 12;
        let num = (super::d(distance) * distance).f();
        let diff = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let sqr = (super::d(diff[0]) * diff[0] + super::d(diff[1]) * diff[1] + super::d(diff[2]) * diff[2]).f();
        let num2 = (super::d(mathf::pow(1.0 - sqr / num, 0.65)) * distance / 2.0).f();
        let mut vertices = [[0.0f32; 3]; 13];
        for i in 0..count {
            let t = i as f32 / count as f32;
            let mut v = [mathf::lerp(a[0], b[0], t), mathf::lerp(a[1], b[1], t), mathf::lerp(a[2], b[2], t)];
            if num2 > mathf::EPSILON {
                let catenary = cosh(2.0 * t * 1.316958 - 1.316958) - 2.0;
                v[1] = (super::d(v[1]) + super::d(catenary) * num2).f();
            }
            vertices[i] = v;
        }
        vertices[0] = a;
        vertices[count] = b;
        self.blood_wires[w].vertices = vertices;
    }

    /// BloodWireBehaviour.Update: DistanceJointWireBehaviour.Update (WireBehaviour.Update, then the vertices), then
    /// ProcessCardiopulmonaryBypassWire.
    pub fn blood_wire_update(&mut self, w: usize) {
        if !self.blood_wires[w].started {
            return;
        }
        let (id, f) = (self.blood_wires[w].id, self.blood_wires[w].joint);
        let other_gone = self.blood_wires[w].other.is_some_and(|o| self.wire_physical_alive(o) && self.physicals[o].isDisintegrated);
        if other_gone || (!self.wire_connected_alive(f) && self.blood_wires[w].usedToHaveConnectedBody) {
            self.blood_wires[w].shouldIgnoreUpdate = true;
            self.tr_destroy_component(id, 0.0);
        } else {
            self.blood_wire_check_mouse_input(w);
        }
        if !self.blood_wires[w].shouldIgnoreUpdate {
            self.blood_wire_calculate_parabola(w);
        }
        self.blood_wire_process_cardiopulmonary_bypass(w);
    }

    /// BloodWireBehaviour.ProcessCardiopulmonaryBypassWire. A destroyed machine or limb throws (Missing/NullReference
    /// exception, logged by Unity), which ends the method.
    fn blood_wire_process_cardiopulmonary_bypass(&mut self, w: usize) {
        if !self.blood_wires[w].IsCardiopulmonaryBypassWire {
            return;
        }
        let Some(c) = self.blood_wires[w].cardiopulmonaryBypass.filter(|&c| self.wire_physical_alive(self.cardiopulmonary_bypasses[c].physical)) else { return };
        let num = mathf::clamp01(self.cardiopulmonary_bypass_final_intensity(c));
        let Some(l) = self.wire_first_limb(w) else { return };
        if self.nodes[l].IsConnectedToRoot {
            let person = self.limbs[l].Person;
            self.people[person].OxygenLevel = mathf::max(0.5 * num, self.people[person].OxygenLevel);
        }
        self.circulations[l].ArtificialHeartbeat = self.cardiopulmonary_bypasses[c].TargetHeartrate * num;
        self.circulations[l].BloodFlow = mathf::max(self.circulations[l].BloodFlow, num);
    }

    /// `limbA ? limbA : limbB` with a destroyed component reading as null.
    fn wire_first_limb(&self, w: usize) -> Option<usize> {
        let alive = |l: Option<usize>| l.filter(|&l| self.wire_physical_alive(self.limbs[l].PhysicalBehaviour));
        alive(self.blood_wires[w].limbA).or_else(|| alive(self.blood_wires[w].limbB))
    }

    /// BloodWireBehaviour.Tick (FixedUpdate)
    pub fn blood_wire_fixed_update(&mut self, w: usize) {
        if !self.blood_wires[w].started {
            return;
        }
        if self.blood_wires[w].IsHeartRateWire {
            self.blood_wire_send_heart_rate(w);
        }
        let dt = self.time.fixedDeltaTime;
        let transfer_rate = dt * 2.0;
        let (a, b) = (self.blood_wires[w].containerA.filter(|&o| self.wire_container_alive(o)), self.blood_wires[w].containerB.filter(|&o| self.wire_container_alive(o)));
        if let (Some(a), Some(b)) = (a, b) {
            if self.wire_allows(a) && self.wire_allows(b) {
                self.blood_wire_transfer(a, b, transfer_rate);
            }
            self.blood_wire_average_pressure(dt, a, b);
        }
        if self.blood_wires[w].AreBothLimbs {
            let (la, lb) = (self.blood_wires[w].limbA.unwrap(), self.blood_wires[w].limbB.unwrap());
            let (fa, fb) = (self.circulations[la].BloodFlow, self.circulations[lb].BloodFlow);
            let mean = 0.5 * (fa + fb);
            if self.wire_allows(Owner::Circulation(la)) && self.wire_allows(Owner::Circulation(lb)) {
                self.circulations[la].BloodFlow = mathf::lerp(fa, mean, dt);
                self.circulations[lb].BloodFlow = mathf::lerp(fb, mean, dt);
            }
        }
        // flow and color ease back (presentation).
    }

    /// BloodWireBehaviour.SendHeartRate
    fn blood_wire_send_heart_rate(&mut self, w: usize) {
        let heart = self.blood_wires[w].heartLimb.unwrap();
        let monitor = self.blood_wires[w].heartMonitor.unwrap();
        let gone = self.physicals[self.limbs[heart].PhysicalBehaviour].isDisintegrated || self.physicals[self.blood_wires[w].physical].isDisintegrated;
        if gone {
            self.heart_monitors[monitor].IsConnected = false;
        } else {
            self.heart_monitors[monitor].IsConnected = true;
            self.heart_monitors[monitor].ConnectedLimb = Some(heart);
        }
    }

    /// BloodWireBehaviour.Transfer: from the container at the higher MeasuredPressure to the other.
    fn blood_wire_transfer(&mut self, a: Owner, b: Owner, rate: f32) {
        let (pa, pb) = (self.bc(a).MeasuredPressure, self.bc(b).MeasuredPressure);
        if pa > pb {
            self.blood_wire_transfer_liquid(rate, a, b);
        } else if pb > pa {
            self.blood_wire_transfer_liquid(rate, b, a);
        }
    }

    /// BloodWireBehaviour.TransferLiquid (a negative rate throws; the rate is fixedDeltaTime * 2).
    fn blood_wire_transfer_liquid(&mut self, rate: f32, from: Owner, to: Owner) {
        if rate < 0.0 {
            unimplemented!("BloodWireBehaviour.TransferLiquid: Rate can't be negative");
        }
        if self.bc(from).TotalLiquidAmount > self.container_limits(from)[0]
            && self.bc(to).TotalLiquidAmount < self.upper_limit(to)
            && self.transfer_to(from, rate, to).abs() > mathf::EPSILON
        {
            self.delete_empty_liquid_entries(from);
            self.delete_empty_liquid_entries(to);
            // color = Color.Lerp(color, from.GetComputedColor(), rate): the call caches the colour.
            let fallback = self.bc(from).cachedComputedColour;
            self.get_computed_color(from, fallback);
        }
    }

    /// BloodWireBehaviour.AveragePressure
    fn blood_wire_average_pressure(&mut self, rate: f32, from: Owner, to: Owner) {
        if rate < 0.0 {
            unimplemented!("BloodWireBehaviour.AveragePressure: Rate can't be negative");
        }
        let (a, b) = (self.bc(from).MeasuredPressure, self.bc(to).MeasuredPressure);
        let mean = 0.5 * (a + b);
        if self.wire_allows(from) && self.wire_allows(to) {
            self.bc_mut(from).MeasuredPressure = mathf::lerp(a, mean, rate);
            self.bc_mut(to).MeasuredPressure = mathf::lerp(b, mean, rate);
        }
    }

    /// Messages.ISlice.Slice of every wire on the physical: breakForce and breakTorque 0.
    pub(super) fn blood_wire_slice(&mut self, p: usize) {
        for w in 0..self.blood_wires.len() {
            if self.blood_wires[w].physical == p && !self.destroyed_components.contains(&self.blood_wires[w].id) {
                let f = self.blood_wires[w].joint;
                self.fixed_joint_set_break_force(f, 0.0);
                self.fixed_joint_set_break_torque(f, 0.0);
            }
        }
    }

    /// WireBehaviour.OnJointBreak2D, sent to the broken joint's GameObject: JointBroken is empty, then Destroy(this).
    pub(super) fn blood_wire_on_joint_break(&mut self, f: usize) {
        for w in 0..self.blood_wires.len() {
            let id = self.blood_wires[w].id;
            if self.blood_wires[w].joint == f && !self.destroyed_components.contains(&id) {
                self.tr_destroy_component(id, 0.0);
            }
        }
    }

    /// The delayed Destroy of a BloodWireBehaviour (false: `id` is not one): RemoveFromManager, its coroutines stop, then
    /// OnDestroy.
    pub fn blood_wire_run_destroy(&mut self, id: i32) -> bool {
        let Some(w) = self.blood_wires.iter().position(|x| x.id == id) else { return false };
        self.component_destroys.retain(|&(_, i)| i != id);
        if self.destroyed_components.insert(id) {
            self.blood_wire_destroy(w);
        }
        true
    }

    /// A wire on a GameObject that is being destroyed: the Joint2D component goes first (it was added before the wire),
    /// then the wire.
    pub(super) fn blood_wire_object_destroy(&mut self, components: &[i32]) {
        for w in 0..self.blood_wires.len() {
            let id = self.blood_wires[w].id;
            if components.contains(&self.physicals[self.blood_wires[w].physical].id) && self.destroyed_components.insert(id) {
                self.component_destroys.retain(|&(_, i)| i != id);
                self.run_joint_destroy(self.blood_wires[w].joint);
                self.blood_wire_destroy(w);
            }
        }
    }

    fn blood_wire_destroy(&mut self, w: usize) {
        let id = self.blood_wires[w].id;
        self.stop_all_coroutines(id);
        if let Some((_, b)) = self.behaviour_of.remove(&id) {
            self.behaviours.remove(b);
        }
        // OnDestroy starts with InvisibilityTracker.Dispose(): a wire whose Start never ran has none (NullReferenceException).
        if self.blood_wires[w].started {
            self.blood_wire_on_destroy(w);
        }
    }

    /// WireBehaviour.OnDestroy, then BloodWireBehaviour.OnDestroy.
    fn blood_wire_on_destroy(&mut self, w: usize) {
        let (id, f) = (self.blood_wires[w].id, self.blood_wires[w].joint);
        // The undo action leaves the history (FindRelevantAction, DeregisterAction); the Wire child and the joint are
        // destroyed (Destroy(this) is a no-op now).
        self.undo_deregister_related(id);
        if !self.fixed_joints[f].destroyed && !self.joint_destroys.iter().any(|&(_, j)| j == f) {
            self.destroy_joint_delayed(f);
        }
        if self.blood_wires[w].IsHeartRateWire {
            if let Some(m) = self.blood_wires[w].heartMonitor.filter(|&m| self.wire_physical_alive(self.heart_monitors[m].physical)) {
                self.heart_monitors[m].IsConnected = false;
            }
        }
        if self.blood_wires[w].IsCardiopulmonaryBypassWire {
            if let Some(c) = self.blood_wires[w].cardiopulmonaryBypass.filter(|&c| self.wire_physical_alive(self.cardiopulmonary_bypasses[c].physical)) {
                // `(limbA ? limbA : limbB).ArtificialHeartbeat = 0` throws when both limbs are destroyed, which ends the method.
                let Some(l) = self.wire_first_limb(w) else { return };
                self.circulations[l].ArtificialHeartbeat = 0.0;
                self.cardiopulmonary_bypasses[c].ValidConnectionCount -= 1;
                let cid = self.cardiopulmonary_bypasses[c].id;
                self.with_component_instance(cid, |g, inst, _| g.cardiopulmonary_bypass_update_activation(c, inst));
            }
        }
    }
}
