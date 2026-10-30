//! libtess2 at commit fc52516 (2019-09-26), the tesselator PolygonCollider2D uses in UnityPlayer.dll: TESSreal is
//! float and the asserts are compiled out. Pointers become arena indices: vertex 0, face 0 and edges 0/1 are the
//! mesh heads, an edge's Sym is its index ^ 1 (an EdgePair's second half has the higher address, so "Sym < e" is
//! "e is odd"), and freed records are only unlinked. Records C takes from its bucket allocator start zeroed here;
//! C leaves the fields MakeVertex/MakeFace don't write as whatever the memory held, which only the degenerate
//! "intersection snapped to the event" splits ever read.

const NIL: usize = usize::MAX;
pub const TESS_UNDEF: i32 = -1;

/// (s, t), what the sweep compares.
type P = (f32, f32);

#[derive(Clone, Copy)]
struct Vertex {
    next: usize,
    prev: usize,
    an_edge: usize,
    coords: [f32; 3],
    s: f32,
    t: f32,
    pq_handle: i32,
    n: i32,
    idx: i32,
}

#[derive(Clone, Copy)]
struct Face {
    next: usize,
    prev: usize,
    an_edge: usize,
    n: i32,
    inside: bool,
}

#[derive(Clone, Copy)]
struct Edge {
    next: usize,
    onext: usize,
    lnext: usize,
    org: usize,
    lface: usize,
    region: usize,
    winding: i32,
}

// geom.c

fn vert_eq(u: P, v: P) -> bool {
    u.0 == v.0 && u.1 == v.1
}
fn vert_leq(u: P, v: P) -> bool {
    u.0 < v.0 || (u.0 == v.0 && u.1 <= v.1)
}
fn tr(u: P) -> P {
    (u.1, u.0)
}
fn trans_leq(u: P, v: P) -> bool {
    vert_leq(tr(u), tr(v))
}
fn edge_eval(u: P, v: P, w: P) -> f32 {
    let gap_l = v.0 - u.0;
    let gap_r = w.0 - v.0;
    if gap_l + gap_r > 0.0 {
        if gap_l < gap_r {
            return (v.1 - u.1) + (u.1 - w.1) * (gap_l / (gap_l + gap_r));
        } else {
            return (v.1 - w.1) + (w.1 - u.1) * (gap_r / (gap_l + gap_r));
        }
    }
    0.0
}
fn edge_sign(u: P, v: P, w: P) -> f32 {
    let gap_l = v.0 - u.0;
    let gap_r = w.0 - v.0;
    if gap_l + gap_r > 0.0 {
        return (v.1 - w.1) * gap_l + (v.1 - u.1) * gap_r;
    }
    0.0
}
fn trans_eval(u: P, v: P, w: P) -> f32 {
    edge_eval(tr(u), tr(v), tr(w))
}
fn trans_sign(u: P, v: P, w: P) -> f32 {
    edge_sign(tr(u), tr(v), tr(w))
}
fn vert_ccw(u: P, v: P, w: P) -> bool {
    (u.0 * (v.1 - w.1) + v.0 * (w.1 - u.1) + w.0 * (u.1 - v.1)) >= 0.0
}
/// The ABS macro (keeps -0).
fn abs(x: f32) -> f32 {
    if x < 0.0 { -x } else { x }
}
fn vert_l1dist(u: P, v: P) -> f32 {
    abs(u.0 - v.0) + abs(u.1 - v.1)
}
fn interpolate(a: f32, x: f32, b: f32, y: f32) -> f32 {
    let a = if a < 0.0 { 0.0 } else { a };
    let b = if b < 0.0 { 0.0 } else { b };
    if a <= b {
        if b == 0.0 { (x + y) / 2.0 } else { x + (y - x) * (a / (a + b)) }
    } else {
        y + (x - y) * (b / (a + b))
    }
}

/// tesedgeIntersect: where edges o1-d1 and o2-d2 cross.
fn edge_intersect(mut o1: P, mut d1: P, mut o2: P, mut d2: P) -> P {
    let mut v = (0.0, 0.0);
    if !vert_leq(o1, d1) { std::mem::swap(&mut o1, &mut d1) }
    if !vert_leq(o2, d2) { std::mem::swap(&mut o2, &mut d2) }
    if !vert_leq(o1, o2) {
        std::mem::swap(&mut o1, &mut o2);
        std::mem::swap(&mut d1, &mut d2);
    }
    if !vert_leq(o2, d1) {
        v.0 = (o2.0 + d1.0) / 2.0;
    } else if vert_leq(d1, d2) {
        let (mut z1, mut z2) = (edge_eval(o1, o2, d1), edge_eval(o2, d1, d2));
        if z1 + z2 < 0.0 { (z1, z2) = (-z1, -z2) }
        v.0 = interpolate(z1, o2.0, z2, d1.0);
    } else {
        let (mut z1, mut z2) = (edge_sign(o1, o2, d1), -edge_sign(o1, d2, d1));
        if z1 + z2 < 0.0 { (z1, z2) = (-z1, -z2) }
        v.0 = interpolate(z1, o2.0, z2, d2.0);
    }
    if !trans_leq(o1, d1) { std::mem::swap(&mut o1, &mut d1) }
    if !trans_leq(o2, d2) { std::mem::swap(&mut o2, &mut d2) }
    if !trans_leq(o1, o2) {
        std::mem::swap(&mut o1, &mut o2);
        std::mem::swap(&mut d1, &mut d2);
    }
    if !trans_leq(o2, d1) {
        v.1 = (o2.1 + d1.1) / 2.0;
    } else if trans_leq(d1, d2) {
        let (mut z1, mut z2) = (trans_eval(o1, o2, d1), trans_eval(o2, d1, d2));
        if z1 + z2 < 0.0 { (z1, z2) = (-z1, -z2) }
        v.1 = interpolate(z1, o2.1, z2, d1.1);
    } else {
        let (mut z1, mut z2) = (trans_sign(o1, o2, d1), -trans_sign(o1, d2, d1));
        if z1 + z2 < 0.0 { (z1, z2) = (-z1, -z2) }
        v.1 = interpolate(z1, o2.1, z2, d2.1);
    }
    v
}

// mesh.c

struct Mesh {
    v: Vec<Vertex>,
    f: Vec<Face>,
    e: Vec<Edge>,
}

impl Mesh {
    fn new() -> Mesh {
        let v = Vertex { next: 0, prev: 0, an_edge: NIL, coords: [0.0; 3], s: 0.0, t: 0.0, pq_handle: 0, n: 0, idx: 0 };
        let f = Face { next: 0, prev: 0, an_edge: NIL, n: 0, inside: false };
        let e = Edge { next: 0, onext: NIL, lnext: NIL, org: NIL, lface: NIL, region: NIL, winding: 0 };
        Mesh { v: vec![v], f: vec![f], e: vec![e, Edge { next: 1, ..e }] }
    }

    fn org(&self, e: usize) -> usize {
        self.e[e].org
    }
    fn dst(&self, e: usize) -> usize {
        self.e[e ^ 1].org
    }
    fn onext(&self, e: usize) -> usize {
        self.e[e].onext
    }
    fn lnext(&self, e: usize) -> usize {
        self.e[e].lnext
    }
    fn oprev(&self, e: usize) -> usize {
        self.e[e ^ 1].lnext
    }
    fn lprev(&self, e: usize) -> usize {
        self.e[e].onext ^ 1
    }
    fn rprev(&self, e: usize) -> usize {
        self.e[e ^ 1].onext
    }
    fn dnext(&self, e: usize) -> usize {
        self.rprev(e) ^ 1
    }
    fn lface(&self, e: usize) -> usize {
        self.e[e].lface
    }
    fn rface(&self, e: usize) -> usize {
        self.e[e ^ 1].lface
    }
    fn p(&self, v: usize) -> P {
        (self.v[v].s, self.v[v].t)
    }
    fn po(&self, e: usize) -> P {
        self.p(self.org(e))
    }
    fn pd(&self, e: usize) -> P {
        self.p(self.dst(e))
    }
    fn goes_left(&self, e: usize) -> bool {
        vert_leq(self.pd(e), self.po(e))
    }
    fn goes_right(&self, e: usize) -> bool {
        vert_leq(self.po(e), self.pd(e))
    }
    fn add_winding(&mut self, dst: usize, src: usize) {
        self.e[dst].winding += self.e[src].winding;
        self.e[dst ^ 1].winding += self.e[src ^ 1].winding;
    }

    fn make_edge_pair(&mut self, e_next: usize) -> usize {
        let (e, e_sym) = (self.e.len(), self.e.len() + 1);
        let e_next = if e_next & 1 == 1 { e_next ^ 1 } else { e_next };
        let e_prev = self.e[e_next ^ 1].next;
        let blank = Edge { next: e_next, onext: e, lnext: e_sym, org: NIL, lface: NIL, region: NIL, winding: 0 };
        self.e.push(blank);
        self.e.push(Edge { next: e_prev, onext: e_sym, lnext: e, ..blank });
        self.e[e_prev ^ 1].next = e;
        self.e[e_next ^ 1].next = e_sym;
        e
    }

    fn splice_raw(&mut self, a: usize, b: usize) {
        let (a_onext, b_onext) = (self.e[a].onext, self.e[b].onext);
        self.e[a_onext ^ 1].lnext = b;
        self.e[b_onext ^ 1].lnext = a;
        self.e[a].onext = b_onext;
        self.e[b].onext = a_onext;
    }

    fn make_vertex(&mut self, e_orig: usize, v_next: usize) {
        let v_new = self.v.len();
        let v_prev = self.v[v_next].prev;
        self.v.push(Vertex { next: v_next, prev: v_prev, an_edge: e_orig, ..self.v[0] });
        self.v[v_prev].next = v_new;
        self.v[v_next].prev = v_new;
        let mut e = e_orig;
        loop {
            self.e[e].org = v_new;
            e = self.e[e].onext;
            if e == e_orig { break }
        }
    }

    fn make_face(&mut self, e_orig: usize, f_next: usize) {
        let f_new = self.f.len();
        let f_prev = self.f[f_next].prev;
        self.f.push(Face { next: f_next, prev: f_prev, an_edge: e_orig, n: 0, inside: self.f[f_next].inside });
        self.f[f_prev].next = f_new;
        self.f[f_next].prev = f_new;
        let mut e = e_orig;
        loop {
            self.e[e].lface = f_new;
            e = self.e[e].lnext;
            if e == e_orig { break }
        }
    }

    fn kill_edge(&mut self, e_del: usize) {
        let e_del = if e_del & 1 == 1 { e_del ^ 1 } else { e_del };
        let e_next = self.e[e_del].next;
        let e_prev = self.e[e_del ^ 1].next;
        self.e[e_next ^ 1].next = e_prev;
        self.e[e_prev ^ 1].next = e_next;
    }

    fn kill_vertex(&mut self, v_del: usize, new_org: usize) {
        let e_start = self.v[v_del].an_edge;
        let mut e = e_start;
        loop {
            self.e[e].org = new_org;
            e = self.e[e].onext;
            if e == e_start { break }
        }
        let (v_prev, v_next) = (self.v[v_del].prev, self.v[v_del].next);
        self.v[v_next].prev = v_prev;
        self.v[v_prev].next = v_next;
    }

    fn kill_face(&mut self, f_del: usize, new_lface: usize) {
        let e_start = self.f[f_del].an_edge;
        let mut e = e_start;
        loop {
            self.e[e].lface = new_lface;
            e = self.e[e].lnext;
            if e == e_start { break }
        }
        let (f_prev, f_next) = (self.f[f_del].prev, self.f[f_del].next);
        self.f[f_next].prev = f_prev;
        self.f[f_prev].next = f_next;
    }

    /// tessMeshMakeEdge
    fn make_edge(&mut self) -> usize {
        let e = self.make_edge_pair(0);
        self.make_vertex(e, 0);
        self.make_vertex(e ^ 1, 0);
        self.make_face(e, 0);
        e
    }

    /// tessMeshSplice
    fn splice(&mut self, e_org: usize, e_dst: usize) {
        if e_org == e_dst { return }
        let mut joining_vertices = false;
        let mut joining_loops = false;
        if self.org(e_dst) != self.org(e_org) {
            joining_vertices = true;
            self.kill_vertex(self.org(e_dst), self.org(e_org));
        }
        if self.lface(e_dst) != self.lface(e_org) {
            joining_loops = true;
            self.kill_face(self.lface(e_dst), self.lface(e_org));
        }
        self.splice_raw(e_dst, e_org);
        if !joining_vertices {
            self.make_vertex(e_dst, self.org(e_org));
            let v = self.org(e_org);
            self.v[v].an_edge = e_org;
        }
        if !joining_loops {
            self.make_face(e_dst, self.lface(e_org));
            let f = self.lface(e_org);
            self.f[f].an_edge = e_org;
        }
    }

    /// tessMeshDelete
    fn delete(&mut self, e_del: usize) {
        let e_del_sym = e_del ^ 1;
        let mut joining_loops = false;
        if self.lface(e_del) != self.rface(e_del) {
            joining_loops = true;
            self.kill_face(self.lface(e_del), self.rface(e_del));
        }
        if self.onext(e_del) == e_del {
            self.kill_vertex(self.org(e_del), NIL);
        } else {
            let (rf, o) = (self.rface(e_del), self.org(e_del));
            self.f[rf].an_edge = self.oprev(e_del);
            self.v[o].an_edge = self.onext(e_del);
            self.splice_raw(e_del, self.oprev(e_del));
            if !joining_loops {
                self.make_face(e_del, self.lface(e_del));
            }
        }
        if self.onext(e_del_sym) == e_del_sym {
            self.kill_vertex(self.org(e_del_sym), NIL);
            self.kill_face(self.lface(e_del_sym), NIL);
        } else {
            let (lf, o) = (self.lface(e_del), self.org(e_del_sym));
            self.f[lf].an_edge = self.oprev(e_del_sym);
            self.v[o].an_edge = self.onext(e_del_sym);
            self.splice_raw(e_del_sym, self.oprev(e_del_sym));
        }
        self.kill_edge(e_del);
    }

    /// tessMeshAddEdgeVertex
    fn add_edge_vertex(&mut self, e_org: usize) -> usize {
        let e_new = self.make_edge_pair(e_org);
        let e_new_sym = e_new ^ 1;
        self.splice_raw(e_new, self.lnext(e_org));
        self.e[e_new].org = self.dst(e_org);
        self.make_vertex(e_new_sym, self.org(e_new));
        let lf = self.lface(e_org);
        self.e[e_new].lface = lf;
        self.e[e_new_sym].lface = lf;
        e_new
    }

    /// tessMeshSplitEdge
    fn split_edge(&mut self, e_org: usize) -> usize {
        let e_new = self.add_edge_vertex(e_org) ^ 1;
        self.splice_raw(e_org ^ 1, self.oprev(e_org ^ 1));
        self.splice_raw(e_org ^ 1, e_new);
        self.e[e_org ^ 1].org = self.org(e_new);
        let d = self.dst(e_new);
        self.v[d].an_edge = e_new ^ 1;
        self.e[e_new ^ 1].lface = self.rface(e_org);
        self.e[e_new].winding = self.e[e_org].winding;
        self.e[e_new ^ 1].winding = self.e[e_org ^ 1].winding;
        e_new
    }

    /// tessMeshConnect
    fn connect(&mut self, e_org: usize, e_dst: usize) -> usize {
        let e_new = self.make_edge_pair(e_org);
        let e_new_sym = e_new ^ 1;
        let mut joining_loops = false;
        if self.lface(e_dst) != self.lface(e_org) {
            joining_loops = true;
            self.kill_face(self.lface(e_dst), self.lface(e_org));
        }
        self.splice_raw(e_new, self.lnext(e_org));
        self.splice_raw(e_new_sym, e_dst);
        self.e[e_new].org = self.dst(e_org);
        self.e[e_new_sym].org = self.org(e_dst);
        let lf = self.lface(e_org);
        self.e[e_new].lface = lf;
        self.e[e_new_sym].lface = lf;
        self.f[lf].an_edge = e_new_sym;
        if !joining_loops {
            self.make_face(e_new, self.lface(e_org));
        }
        e_new
    }

    fn count_face_verts(&self, f: usize) -> usize {
        let mut e = self.f[f].an_edge;
        let mut n = 0;
        loop {
            n += 1;
            e = self.lnext(e);
            if e == self.f[f].an_edge { return n }
        }
    }

    /// tessMeshMergeConvexFaces
    fn merge_convex_faces(&mut self, max_verts_per_face: usize) {
        let mut e = self.e[0].next;
        while e != 0 {
            let mut e_next = self.e[e].next;
            let e_sym = e ^ 1;
            let inside = |f: usize| f != NIL && self.f[f].inside;
            if inside(self.lface(e)) && inside(self.lface(e_sym)) {
                let left_nv = self.count_face_verts(self.lface(e));
                let right_nv = self.count_face_verts(self.lface(e_sym));
                if left_nv + right_nv - 2 <= max_verts_per_face {
                    let va = self.po(self.lprev(e));
                    let vb = self.po(e);
                    let vc = self.pd(self.lnext(e_sym));
                    let vd = self.po(self.lprev(e_sym));
                    let ve = self.po(e_sym);
                    let vf = self.pd(self.lnext(e));
                    if vert_ccw(va, vb, vc) && vert_ccw(vd, ve, vf) {
                        if e == e_next || e == e_next ^ 1 {
                            e_next = self.e[e_next].next;
                        }
                        self.delete(e);
                    }
                }
            }
            e = e_next;
        }
    }

    /// tessMeshTessellateMonoRegion
    fn tessellate_mono_region(&mut self, face: usize) {
        let mut up = self.f[face].an_edge;
        while vert_leq(self.pd(up), self.po(up)) {
            up = self.lprev(up);
        }
        while vert_leq(self.po(up), self.pd(up)) {
            up = self.lnext(up);
        }
        let mut lo = self.lprev(up);
        while self.lnext(up) != lo {
            if vert_leq(self.pd(up), self.po(lo)) {
                while self.lnext(lo) != up
                    && (self.goes_left(self.lnext(lo)) || edge_sign(self.po(lo), self.pd(lo), self.pd(self.lnext(lo))) <= 0.0)
                {
                    lo = self.connect(self.lnext(lo), lo) ^ 1;
                }
                lo = self.lprev(lo);
            } else {
                while self.lnext(lo) != up
                    && (self.goes_right(self.lprev(up)) || edge_sign(self.pd(up), self.po(up), self.po(self.lprev(up))) >= 0.0)
                {
                    up = self.connect(up, self.lprev(up)) ^ 1;
                }
                up = self.lnext(up);
            }
        }
        while self.lnext(self.lnext(lo)) != up {
            lo = self.connect(self.lnext(lo), lo) ^ 1;
        }
    }

    /// tessMeshTessellateInterior
    fn tessellate_interior(&mut self) {
        let mut f = self.f[0].next;
        while f != 0 {
            let next = self.f[f].next;
            if self.f[f].inside {
                self.tessellate_mono_region(f);
            }
            f = next;
        }
    }
}

// priorityq.c: keys are vertex indices, NIL for NULL; LEQ is VertLeq.

fn leq(v: &[Vertex], x: usize, y: usize) -> bool {
    vert_leq((v[x].s, v[x].t), (v[y].s, v[y].t))
}

struct Heap {
    /// nodes[i].handle
    nodes: Vec<i32>,
    /// handles[h]: (key, node)
    handles: Vec<(usize, i32)>,
    size: i32,
    max: i32,
    initialized: bool,
    free_list: i32,
}

impl Heap {
    fn new(size: i32) -> Heap {
        let mut h = Heap { nodes: vec![0; size as usize + 1], handles: vec![(NIL, 0); size as usize + 1], size: 0, max: size, initialized: false, free_list: 0 };
        h.nodes[1] = 1;
        h.handles[1].0 = NIL;
        h
    }

    fn key(&self, node: i32) -> usize {
        self.handles[self.nodes[node as usize] as usize].0
    }

    fn float_down(&mut self, v: &[Vertex], mut curr: i32) {
        let h_curr = self.nodes[curr as usize];
        loop {
            let mut child = curr << 1;
            if child < self.size && leq(v, self.key(child + 1), self.key(child)) {
                child += 1;
            }
            let h_child = self.nodes[child as usize];
            if child > self.size || leq(v, self.handles[h_curr as usize].0, self.handles[h_child as usize].0) {
                self.nodes[curr as usize] = h_curr;
                self.handles[h_curr as usize].1 = curr;
                break;
            }
            self.nodes[curr as usize] = h_child;
            self.handles[h_child as usize].1 = curr;
            curr = child;
        }
    }

    fn float_up(&mut self, v: &[Vertex], mut curr: i32) {
        let h_curr = self.nodes[curr as usize];
        loop {
            let parent = curr >> 1;
            let h_parent = self.nodes[parent as usize];
            if parent == 0 || leq(v, self.handles[h_parent as usize].0, self.handles[h_curr as usize].0) {
                self.nodes[curr as usize] = h_curr;
                self.handles[h_curr as usize].1 = curr;
                break;
            }
            self.nodes[curr as usize] = h_parent;
            self.handles[h_parent as usize].1 = curr;
            curr = parent;
        }
    }

    fn init(&mut self, v: &[Vertex]) {
        for i in (1..=self.size).rev() {
            self.float_down(v, i);
        }
        self.initialized = true;
    }

    fn insert(&mut self, v: &[Vertex], key_new: usize) -> i32 {
        self.size += 1;
        let curr = self.size;
        if curr * 2 > self.max {
            self.max <<= 1;
            self.nodes.resize(self.max as usize + 1, 0);
            self.handles.resize(self.max as usize + 1, (NIL, 0));
        }
        let free = if self.free_list == 0 {
            curr
        } else {
            let free = self.free_list;
            self.free_list = self.handles[free as usize].1;
            free
        };
        self.nodes[curr as usize] = free;
        self.handles[free as usize] = (key_new, curr);
        if self.initialized {
            self.float_up(v, curr);
        }
        free
    }

    fn minimum(&self) -> usize {
        self.key(1)
    }

    fn extract_min(&mut self, v: &[Vertex]) -> usize {
        let h_min = self.nodes[1];
        let min = self.handles[h_min as usize].0;
        if self.size > 0 {
            self.nodes[1] = self.nodes[self.size as usize];
            self.handles[self.nodes[1] as usize].1 = 1;
            self.handles[h_min as usize] = (NIL, self.free_list);
            self.free_list = h_min;
            self.size -= 1;
            if self.size > 0 {
                self.float_down(v, 1);
            }
        }
        min
    }

    fn delete(&mut self, v: &[Vertex], h_curr: i32) {
        let curr = self.handles[h_curr as usize].1;
        self.nodes[curr as usize] = self.nodes[self.size as usize];
        self.handles[self.nodes[curr as usize] as usize].1 = curr;
        self.size -= 1;
        if curr <= self.size {
            if curr <= 1 || leq(v, self.key(curr >> 1), self.key(curr)) {
                self.float_down(v, curr);
            } else {
                self.float_up(v, curr);
            }
        }
        self.handles[h_curr as usize] = (NIL, self.free_list);
        self.free_list = h_curr;
    }
}

struct PriorityQ {
    heap: Heap,
    keys: Vec<usize>,
    /// Indices into `keys`.
    order: Vec<usize>,
    size: i32,
    max: i32,
    initialized: bool,
}

impl PriorityQ {
    fn new(size: i32) -> PriorityQ {
        PriorityQ { heap: Heap::new(size), keys: vec![NIL; size as usize], order: Vec::new(), size: 0, max: size, initialized: false }
    }

    fn sorted(&self, i: i32) -> usize {
        self.keys[self.order[i as usize]]
    }

    /// pqInit: sorts the keys inserted so far (quicksort with libtess2's own random pivots, insertion sort for
    /// runs of 10 or fewer) into `order`, largest first.
    fn init(&mut self, v: &[Vertex]) {
        let lt = |x: usize, y: usize| !leq(v, y, x);
        let gt = |x: usize, y: usize| !leq(v, x, y);
        let mut seed: u32 = 2016473283;
        self.order = (0..=self.size as usize).collect();
        let keys = &self.keys;
        let order = &mut self.order;
        let mut stack: Vec<(isize, isize)> = vec![(0, self.size as isize - 1)];
        while let Some((mut p, mut r)) = stack.pop() {
            while r > p + 10 {
                seed = seed.wrapping_mul(1539415821).wrapping_add(1);
                let i = p + (seed as i64 % (r - p + 1) as i64) as isize;
                let piv = order[i as usize];
                order[i as usize] = order[p as usize];
                order[p as usize] = piv;
                let (mut i, mut j) = (p - 1, r + 1);
                loop {
                    loop {
                        i += 1;
                        if !gt(keys[order[i as usize]], keys[piv]) { break }
                    }
                    loop {
                        j -= 1;
                        if !lt(keys[order[j as usize]], keys[piv]) { break }
                    }
                    order.swap(i as usize, j as usize);
                    if i >= j { break }
                }
                order.swap(i as usize, j as usize);
                if i - p < r - j {
                    stack.push((j + 1, r));
                    r = i - 1;
                } else {
                    stack.push((p, i - 1));
                    p = j + 1;
                }
            }
            let mut i = p + 1;
            while i <= r {
                let piv = order[i as usize];
                let mut j = i;
                while j > p && lt(keys[order[j as usize - 1]], keys[piv]) {
                    order[j as usize] = order[j as usize - 1];
                    j -= 1;
                }
                order[j as usize] = piv;
                i += 1;
            }
        }
        self.max = self.size;
        self.initialized = true;
        self.heap.init(v);
    }

    fn insert(&mut self, v: &[Vertex], key_new: usize) -> i32 {
        if self.initialized {
            return self.heap.insert(v, key_new);
        }
        let curr = self.size;
        self.size += 1;
        if self.size >= self.max {
            self.max <<= 1;
            self.keys.resize(self.max as usize, NIL);
        }
        self.keys[curr as usize] = key_new;
        -(curr + 1)
    }

    fn extract_min(&mut self, v: &[Vertex]) -> usize {
        if self.size == 0 {
            return self.heap.extract_min(v);
        }
        let sort_min = self.sorted(self.size - 1);
        if self.heap.size != 0 && leq(v, self.heap.minimum(), sort_min) {
            return self.heap.extract_min(v);
        }
        loop {
            self.size -= 1;
            if !(self.size > 0 && self.sorted(self.size - 1) == NIL) { break }
        }
        sort_min
    }

    fn minimum(&self, v: &[Vertex]) -> usize {
        if self.size == 0 {
            return self.heap.minimum();
        }
        let sort_min = self.sorted(self.size - 1);
        if self.heap.size != 0 && leq(v, self.heap.minimum(), sort_min) {
            return self.heap.minimum();
        }
        sort_min
    }

    fn delete(&mut self, v: &[Vertex], curr: i32) {
        if curr >= 0 {
            self.heap.delete(v, curr);
            return;
        }
        self.keys[-(curr + 1) as usize] = NIL;
        while self.size > 0 && self.sorted(self.size - 1) == NIL {
            self.size -= 1;
        }
    }
}

// sweep.c

#[derive(Clone, Copy)]
struct Region {
    e_up: usize,
    node_up: usize,
    winding_number: i32,
    inside: bool,
    dirty: bool,
    fix_upper_edge: bool,
}

#[derive(Clone, Copy)]
struct DictNode {
    key: usize,
    next: usize,
    prev: usize,
}

struct Tess {
    m: Mesh,
    bmin: [f32; 2],
    bmax: [f32; 2],
    /// Node 0 is the head; keys are region indices.
    dict: Vec<DictNode>,
    regions: Vec<Region>,
    pq: PriorityQ,
    event: usize,
}

impl Tess {
    fn below(&self, r: usize) -> usize {
        self.dict[self.dict[self.regions[r].node_up].prev].key
    }
    fn above(&self, r: usize) -> usize {
        self.dict[self.dict[self.regions[r].node_up].next].key
    }
    fn up(&self, r: usize) -> usize {
        self.regions[r].e_up
    }

    /// EdgeLeq on the regions' upper edges.
    fn edge_leq(&self, e1: usize, e2: usize) -> bool {
        let m = &self.m;
        let event = self.event;
        let ev = m.p(event);
        if m.dst(e1) == event {
            if m.dst(e2) == event {
                if vert_leq(m.po(e1), m.po(e2)) {
                    return edge_sign(m.pd(e2), m.po(e1), m.po(e2)) <= 0.0;
                }
                return edge_sign(m.pd(e1), m.po(e2), m.po(e1)) >= 0.0;
            }
            return edge_sign(m.pd(e2), ev, m.po(e2)) <= 0.0;
        }
        if m.dst(e2) == event {
            return edge_sign(m.pd(e1), ev, m.po(e1)) >= 0.0;
        }
        let t1 = edge_eval(m.pd(e1), ev, m.po(e1));
        let t2 = edge_eval(m.pd(e2), ev, m.po(e2));
        t1 >= t2
    }

    fn dict_insert_before(&mut self, mut node: usize, key: usize) -> usize {
        loop {
            node = self.dict[node].prev;
            let k = self.dict[node].key;
            if k == NIL || self.edge_leq(self.up(k), self.up(key)) { break }
        }
        let new = self.dict.len();
        let next = self.dict[node].next;
        self.dict.push(DictNode { key, next, prev: node });
        self.dict[next].prev = new;
        self.dict[node].next = new;
        new
    }

    fn dict_search(&self, e_up: usize) -> usize {
        let mut node = 0;
        loop {
            node = self.dict[node].next;
            let k = self.dict[node].key;
            if k == NIL || self.edge_leq(e_up, self.up(k)) { return node }
        }
    }

    fn dict_delete(&mut self, node: usize) {
        let DictNode { next, prev, .. } = self.dict[node];
        self.dict[next].prev = prev;
        self.dict[prev].next = next;
    }

    fn delete_region(&mut self, reg: usize) {
        let e = self.up(reg);
        self.m.e[e].region = NIL;
        self.dict_delete(self.regions[reg].node_up);
    }

    fn fix_upper_edge(&mut self, reg: usize, new_edge: usize) {
        self.m.delete(self.up(reg));
        self.regions[reg].fix_upper_edge = false;
        self.regions[reg].e_up = new_edge;
        self.m.e[new_edge].region = reg;
    }

    fn top_left_region(&mut self, mut reg: usize) -> usize {
        let org = self.m.org(self.up(reg));
        loop {
            reg = self.above(reg);
            if self.m.org(self.up(reg)) != org { break }
        }
        if self.regions[reg].fix_upper_edge {
            let e = self.m.connect(self.up(self.below(reg)) ^ 1, self.m.lnext(self.up(reg)));
            self.fix_upper_edge(reg, e);
            reg = self.above(reg);
        }
        reg
    }

    fn top_right_region(&self, mut reg: usize) -> usize {
        let dst = self.m.dst(self.up(reg));
        loop {
            reg = self.above(reg);
            if self.m.dst(self.up(reg)) != dst { return reg }
        }
    }

    fn add_region_below(&mut self, reg_above: usize, e_new_up: usize) -> usize {
        let reg_new = self.regions.len();
        self.regions.push(Region { e_up: e_new_up, node_up: NIL, winding_number: 0, inside: false, dirty: false, fix_upper_edge: false });
        self.regions[reg_new].node_up = self.dict_insert_before(self.regions[reg_above].node_up, reg_new);
        self.m.e[e_new_up].region = reg_new;
        reg_new
    }

    /// IsWindingInside for TESS_WINDING_ODD, the rule Unity tesselates with.
    fn is_winding_inside(&self, n: i32) -> bool {
        n & 1 != 0
    }

    fn compute_winding(&mut self, reg: usize) {
        let n = self.regions[self.above(reg)].winding_number + self.m.e[self.up(reg)].winding;
        self.regions[reg].winding_number = n;
        self.regions[reg].inside = self.is_winding_inside(n);
    }

    fn finish_region(&mut self, reg: usize) {
        let e = self.up(reg);
        let f = self.m.lface(e);
        self.m.f[f].inside = self.regions[reg].inside;
        self.m.f[f].an_edge = e;
        self.delete_region(reg);
    }

    fn finish_left_regions(&mut self, reg_first: usize, reg_last: usize) -> usize {
        let mut reg_prev = reg_first;
        let mut e_prev = self.up(reg_first);
        while reg_prev != reg_last {
            self.regions[reg_prev].fix_upper_edge = false;
            let reg = self.below(reg_prev);
            let mut e = self.up(reg);
            if self.m.org(e) != self.m.org(e_prev) {
                if !self.regions[reg].fix_upper_edge {
                    self.finish_region(reg_prev);
                    break;
                }
                e = self.m.connect(self.m.lprev(e_prev), e ^ 1);
                self.fix_upper_edge(reg, e);
            }
            if self.m.onext(e_prev) != e {
                self.m.splice(self.m.oprev(e), e);
                self.m.splice(e_prev, e);
            }
            self.finish_region(reg_prev);
            e_prev = self.up(reg);
            reg_prev = reg;
        }
        e_prev
    }

    fn add_right_edges(&mut self, reg_up: usize, e_first: usize, e_last: usize, mut e_top_left: usize, clean_up: bool) {
        let mut e = e_first;
        loop {
            self.add_region_below(reg_up, e ^ 1);
            e = self.m.onext(e);
            if e == e_last { break }
        }
        if e_top_left == NIL {
            e_top_left = self.m.rprev(self.up(self.below(reg_up)));
        }
        let mut reg_prev = reg_up;
        let mut e_prev = e_top_left;
        let mut first_time = true;
        loop {
            let reg = self.below(reg_prev);
            e = self.up(reg) ^ 1;
            if self.m.org(e) != self.m.org(e_prev) { break }
            if self.m.onext(e) != e_prev {
                self.m.splice(self.m.oprev(e), e);
                self.m.splice(self.m.oprev(e_prev), e);
            }
            let n = self.regions[reg_prev].winding_number - self.m.e[e].winding;
            self.regions[reg].winding_number = n;
            self.regions[reg].inside = self.is_winding_inside(n);
            self.regions[reg_prev].dirty = true;
            if !first_time && self.check_for_right_splice(reg_prev) {
                self.m.add_winding(e, e_prev);
                self.delete_region(reg_prev);
                self.m.delete(e_prev);
            }
            first_time = false;
            reg_prev = reg;
            e_prev = e;
        }
        self.regions[reg_prev].dirty = true;
        if clean_up {
            self.walk_dirty_regions(reg_prev);
        }
    }

    /// GetIntersectData: the new vertex's coords as weights of the two edges' ends.
    fn get_intersect_data(&mut self, isect: usize, org_up: usize, dst_up: usize, org_lo: usize, dst_lo: usize) {
        self.m.v[isect].coords = [0.0; 3];
        self.m.v[isect].idx = TESS_UNDEF;
        for (org, dst) in [(org_up, dst_up), (org_lo, dst_lo)] {
            let m = &mut self.m;
            let t1 = vert_l1dist(m.p(org), m.p(isect));
            let t2 = vert_l1dist(m.p(dst), m.p(isect));
            let w0 = 0.5 * t2 / (t1 + t2);
            let w1 = 0.5 * t1 / (t1 + t2);
            let (o, d) = (m.v[org].coords, m.v[dst].coords);
            for i in 0..3 {
                m.v[isect].coords[i] += w0 * o[i] + w1 * d[i];
            }
        }
    }

    fn check_for_right_splice(&mut self, reg_up: usize) -> bool {
        let reg_lo = self.below(reg_up);
        let e_up = self.up(reg_up);
        let e_lo = self.up(reg_lo);
        if vert_leq(self.m.po(e_up), self.m.po(e_lo)) {
            if edge_sign(self.m.pd(e_lo), self.m.po(e_up), self.m.po(e_lo)) > 0.0 {
                return false;
            }
            if !vert_eq(self.m.po(e_up), self.m.po(e_lo)) {
                self.m.split_edge(e_lo ^ 1);
                self.m.splice(e_up, self.m.oprev(e_lo));
                self.regions[reg_up].dirty = true;
                self.regions[reg_lo].dirty = true;
            } else if self.m.org(e_up) != self.m.org(e_lo) {
                let h = self.m.v[self.m.org(e_up)].pq_handle;
                self.pq.delete(&self.m.v, h);
                self.m.splice(self.m.oprev(e_lo), e_up);
            }
        } else {
            if edge_sign(self.m.pd(e_up), self.m.po(e_lo), self.m.po(e_up)) <= 0.0 {
                return false;
            }
            let above = self.above(reg_up);
            self.regions[above].dirty = true;
            self.regions[reg_up].dirty = true;
            self.m.split_edge(e_up ^ 1);
            self.m.splice(self.m.oprev(e_lo), e_up);
        }
        true
    }

    fn check_for_left_splice(&mut self, reg_up: usize) -> bool {
        let reg_lo = self.below(reg_up);
        let e_up = self.up(reg_up);
        let e_lo = self.up(reg_lo);
        if vert_leq(self.m.pd(e_up), self.m.pd(e_lo)) {
            if edge_sign(self.m.pd(e_up), self.m.pd(e_lo), self.m.po(e_up)) < 0.0 {
                return false;
            }
            let above = self.above(reg_up);
            self.regions[above].dirty = true;
            self.regions[reg_up].dirty = true;
            let e = self.m.split_edge(e_up);
            self.m.splice(e_lo ^ 1, e);
            let f = self.m.lface(e);
            self.m.f[f].inside = self.regions[reg_up].inside;
        } else {
            if edge_sign(self.m.pd(e_lo), self.m.pd(e_up), self.m.po(e_lo)) > 0.0 {
                return false;
            }
            self.regions[reg_up].dirty = true;
            self.regions[reg_lo].dirty = true;
            let e = self.m.split_edge(e_lo);
            self.m.splice(self.m.lnext(e_up), e_lo ^ 1);
            let f = self.m.rface(e);
            self.m.f[f].inside = self.regions[reg_up].inside;
        }
        true
    }

    fn check_for_intersect(&mut self, mut reg_up: usize) -> bool {
        let mut reg_lo = self.below(reg_up);
        let e_up = self.up(reg_up);
        let mut e_lo = self.up(reg_lo);
        let (org_up, org_lo) = (self.m.org(e_up), self.m.org(e_lo));
        let (dst_up, dst_lo) = (self.m.dst(e_up), self.m.dst(e_lo));
        let p = |v: usize| self.m.p(v);
        let (po_up, po_lo, pd_up, pd_lo, ev) = (p(org_up), p(org_lo), p(dst_up), p(dst_lo), p(self.event));
        if org_up == org_lo {
            return false;
        }
        let t_min_up = if po_up.1 <= pd_up.1 { po_up.1 } else { pd_up.1 };
        let t_max_lo = if po_lo.1 >= pd_lo.1 { po_lo.1 } else { pd_lo.1 };
        if t_min_up > t_max_lo {
            return false;
        }
        if vert_leq(po_up, po_lo) {
            if edge_sign(pd_lo, po_up, po_lo) > 0.0 {
                return false;
            }
        } else if edge_sign(pd_up, po_lo, po_up) < 0.0 {
            return false;
        }
        let mut isect = edge_intersect(pd_up, po_up, pd_lo, po_lo);
        if vert_leq(isect, ev) {
            isect = ev;
        }
        let org_min = if vert_leq(po_up, po_lo) { po_up } else { po_lo };
        if vert_leq(org_min, isect) {
            isect = org_min;
        }
        if vert_eq(isect, po_up) || vert_eq(isect, po_lo) {
            self.check_for_right_splice(reg_up);
            return false;
        }
        if (!vert_eq(pd_up, ev) && edge_sign(pd_up, ev, isect) >= 0.0) || (!vert_eq(pd_lo, ev) && edge_sign(pd_lo, ev, isect) <= 0.0) {
            if dst_lo == self.event {
                self.m.split_edge(e_up ^ 1);
                self.m.splice(e_lo ^ 1, e_up);
                reg_up = self.top_left_region(reg_up);
                let e_up = self.up(self.below(reg_up));
                self.finish_left_regions(self.below(reg_up), reg_lo);
                self.add_right_edges(reg_up, self.m.oprev(e_up), e_up, e_up, true);
                return true;
            }
            if dst_up == self.event {
                self.m.split_edge(e_lo ^ 1);
                self.m.splice(self.m.lnext(e_up), self.m.oprev(e_lo));
                reg_lo = reg_up;
                reg_up = self.top_right_region(reg_up);
                let e = self.m.rprev(self.up(self.below(reg_up)));
                self.regions[reg_lo].e_up = self.m.oprev(e_lo);
                e_lo = self.finish_left_regions(reg_lo, NIL);
                self.add_right_edges(reg_up, self.m.onext(e_lo), self.m.rprev(e_up), e, true);
                return true;
            }
            if edge_sign(pd_up, ev, isect) >= 0.0 {
                let above = self.above(reg_up);
                self.regions[above].dirty = true;
                self.regions[reg_up].dirty = true;
                self.m.split_edge(e_up ^ 1);
                let o = self.m.org(e_up);
                (self.m.v[o].s, self.m.v[o].t) = ev;
            }
            if edge_sign(pd_lo, ev, isect) <= 0.0 {
                self.regions[reg_up].dirty = true;
                self.regions[reg_lo].dirty = true;
                self.m.split_edge(e_lo ^ 1);
                let o = self.m.org(e_lo);
                (self.m.v[o].s, self.m.v[o].t) = ev;
            }
            return false;
        }
        self.m.split_edge(e_up ^ 1);
        self.m.split_edge(e_lo ^ 1);
        self.m.splice(self.m.oprev(e_lo), e_up);
        let o = self.m.org(e_up);
        (self.m.v[o].s, self.m.v[o].t) = isect;
        self.m.v[o].pq_handle = self.pq.insert(&self.m.v, o);
        self.get_intersect_data(o, org_up, dst_up, org_lo, dst_lo);
        let above = self.above(reg_up);
        self.regions[above].dirty = true;
        self.regions[reg_up].dirty = true;
        self.regions[reg_lo].dirty = true;
        false
    }

    fn walk_dirty_regions(&mut self, mut reg_up: usize) {
        let mut reg_lo = self.below(reg_up);
        loop {
            while self.regions[reg_lo].dirty {
                reg_up = reg_lo;
                reg_lo = self.below(reg_lo);
            }
            if !self.regions[reg_up].dirty {
                reg_lo = reg_up;
                reg_up = self.above(reg_up);
                if reg_up == NIL || !self.regions[reg_up].dirty {
                    return;
                }
            }
            self.regions[reg_up].dirty = false;
            let mut e_up = self.up(reg_up);
            let mut e_lo = self.up(reg_lo);
            if self.m.dst(e_up) != self.m.dst(e_lo) && self.check_for_left_splice(reg_up) {
                if self.regions[reg_lo].fix_upper_edge {
                    self.delete_region(reg_lo);
                    self.m.delete(e_lo);
                    reg_lo = self.below(reg_up);
                    e_lo = self.up(reg_lo);
                } else if self.regions[reg_up].fix_upper_edge {
                    self.delete_region(reg_up);
                    self.m.delete(e_up);
                    reg_up = self.above(reg_lo);
                    e_up = self.up(reg_up);
                }
            }
            if self.m.org(e_up) != self.m.org(e_lo) {
                if self.m.dst(e_up) != self.m.dst(e_lo)
                    && !self.regions[reg_up].fix_upper_edge
                    && !self.regions[reg_lo].fix_upper_edge
                    && (self.m.dst(e_up) == self.event || self.m.dst(e_lo) == self.event)
                {
                    if self.check_for_intersect(reg_up) {
                        return;
                    }
                } else {
                    self.check_for_right_splice(reg_up);
                }
            }
            if self.m.org(e_up) == self.m.org(e_lo) && self.m.dst(e_up) == self.m.dst(e_lo) {
                self.m.add_winding(e_lo, e_up);
                self.delete_region(reg_up);
                self.m.delete(e_up);
                reg_up = self.above(reg_lo);
            }
        }
    }

    fn connect_right_vertex(&mut self, mut reg_up: usize, mut e_bottom_left: usize) {
        let mut e_top_left = self.m.onext(e_bottom_left);
        let reg_lo = self.below(reg_up);
        let e_up = self.up(reg_up);
        let e_lo = self.up(reg_lo);
        let mut degenerate = false;
        if self.m.dst(e_up) != self.m.dst(e_lo) {
            self.check_for_intersect(reg_up);
        }
        if vert_eq(self.m.po(e_up), self.m.p(self.event)) {
            self.m.splice(self.m.oprev(e_top_left), e_up);
            reg_up = self.top_left_region(reg_up);
            e_top_left = self.up(self.below(reg_up));
            self.finish_left_regions(self.below(reg_up), reg_lo);
            degenerate = true;
        }
        if vert_eq(self.m.po(e_lo), self.m.p(self.event)) {
            self.m.splice(e_bottom_left, self.m.oprev(e_lo));
            e_bottom_left = self.finish_left_regions(reg_lo, NIL);
            degenerate = true;
        }
        if degenerate {
            self.add_right_edges(reg_up, self.m.onext(e_bottom_left), e_top_left, e_top_left, true);
            return;
        }
        let e_new = if vert_leq(self.m.po(e_lo), self.m.po(e_up)) { self.m.oprev(e_lo) } else { e_up };
        let e_new = self.m.connect(self.m.lprev(e_bottom_left), e_new);
        self.add_right_edges(reg_up, e_new, self.m.onext(e_new), self.m.onext(e_new), false);
        let r = self.m.e[e_new ^ 1].region;
        self.regions[r].fix_upper_edge = true;
        self.walk_dirty_regions(reg_up);
    }

    fn connect_left_degenerate(&mut self, mut reg_up: usize, v_event: usize) {
        let e = self.up(reg_up);
        if vert_eq(self.m.po(e), self.m.p(v_event)) {
            // Unreachable with TOLERANCE_NONZERO false (an assert in libtess2).
            self.m.splice(e, self.m.v[v_event].an_edge);
            return;
        }
        if !vert_eq(self.m.pd(e), self.m.p(v_event)) {
            self.m.split_edge(e ^ 1);
            if self.regions[reg_up].fix_upper_edge {
                self.m.delete(self.m.onext(e));
                self.regions[reg_up].fix_upper_edge = false;
            }
            self.m.splice(self.m.v[v_event].an_edge, e);
            self.sweep_event(v_event);
            return;
        }
        // Unreachable with TOLERANCE_NONZERO false.
        reg_up = self.top_right_region(reg_up);
        let reg = self.below(reg_up);
        let mut e_top_right = self.up(reg) ^ 1;
        let mut e_top_left = self.m.onext(e_top_right);
        let e_last = e_top_left;
        if self.regions[reg].fix_upper_edge {
            self.delete_region(reg);
            self.m.delete(e_top_right);
            e_top_right = self.m.oprev(e_top_left);
        }
        self.m.splice(self.m.v[v_event].an_edge, e_top_right);
        if !self.m.goes_left(e_top_left) {
            e_top_left = NIL;
        }
        self.add_right_edges(reg_up, self.m.onext(e_top_right), e_last, e_top_left, true);
    }

    fn connect_left_vertex(&mut self, v_event: usize) {
        let reg_up = self.dict[self.dict_search(self.m.v[v_event].an_edge ^ 1)].key;
        let reg_lo = self.below(reg_up);
        if reg_lo == NIL {
            return;
        }
        let e_up = self.up(reg_up);
        let e_lo = self.up(reg_lo);
        if edge_sign(self.m.pd(e_up), self.m.p(v_event), self.m.po(e_up)) == 0.0 {
            self.connect_left_degenerate(reg_up, v_event);
            return;
        }
        let reg = if vert_leq(self.m.pd(e_lo), self.m.pd(e_up)) { reg_up } else { reg_lo };
        if self.regions[reg_up].inside || self.regions[reg].fix_upper_edge {
            let e_new = if reg == reg_up {
                self.m.connect(self.m.v[v_event].an_edge ^ 1, self.m.lnext(e_up))
            } else {
                self.m.connect(self.m.dnext(e_lo), self.m.v[v_event].an_edge) ^ 1
            };
            if self.regions[reg].fix_upper_edge {
                self.fix_upper_edge(reg, e_new);
            } else {
                let r = self.add_region_below(reg_up, e_new);
                self.compute_winding(r);
            }
            self.sweep_event(v_event);
        } else {
            let a = self.m.v[v_event].an_edge;
            self.add_right_edges(reg_up, a, a, NIL, true);
        }
    }

    fn sweep_event(&mut self, v_event: usize) {
        self.event = v_event;
        let mut e = self.m.v[v_event].an_edge;
        while self.m.e[e].region == NIL {
            e = self.m.onext(e);
            if e == self.m.v[v_event].an_edge {
                self.connect_left_vertex(v_event);
                return;
            }
        }
        let reg_up = self.top_left_region(self.m.e[e].region);
        let reg = self.below(reg_up);
        let e_top_left = self.up(reg);
        let e_bottom_left = self.finish_left_regions(reg, NIL);
        if self.m.onext(e_bottom_left) == e_top_left {
            self.connect_right_vertex(reg_up, e_bottom_left);
        } else {
            self.add_right_edges(reg_up, self.m.onext(e_bottom_left), e_top_left, e_top_left, true);
        }
    }

    fn add_sentinel(&mut self, smin: f32, smax: f32, t: f32) {
        let e = self.m.make_edge();
        let (o, d) = (self.m.org(e), self.m.dst(e));
        (self.m.v[o].s, self.m.v[o].t) = (smax, t);
        (self.m.v[d].s, self.m.v[d].t) = (smin, t);
        self.event = d;
        let reg = self.regions.len();
        self.regions.push(Region { e_up: e, node_up: NIL, winding_number: 0, inside: false, dirty: false, fix_upper_edge: false });
        self.regions[reg].node_up = self.dict_insert_before(0, reg);
    }

    fn init_edge_dict(&mut self) {
        self.dict = vec![DictNode { key: NIL, next: 0, prev: 0 }];
        let w = (self.bmax[0] - self.bmin[0]) + 0.01;
        let h = (self.bmax[1] - self.bmin[1]) + 0.01;
        let (smin, smax) = (self.bmin[0] - w, self.bmax[0] + w);
        let (tmin, tmax) = (self.bmin[1] - h, self.bmax[1] + h);
        self.add_sentinel(smin, smax, tmin);
        self.add_sentinel(smin, smax, tmax);
    }

    fn done_edge_dict(&mut self) {
        loop {
            let reg = self.dict[self.dict[0].next].key;
            if reg == NIL { break }
            self.delete_region(reg);
        }
    }

    fn remove_degenerate_edges(&mut self) {
        let m = &mut self.m;
        let mut e = m.e[0].next;
        while e != 0 {
            let mut e_next = m.e[e].next;
            let mut e_lnext = m.lnext(e);
            if vert_eq(m.po(e), m.pd(e)) && m.lnext(m.lnext(e)) != e {
                m.splice(e_lnext, e);
                m.delete(e);
                e = e_lnext;
                e_lnext = m.lnext(e);
            }
            if m.lnext(e_lnext) == e {
                if e_lnext != e {
                    if e_lnext == e_next || e_lnext == e_next ^ 1 {
                        e_next = m.e[e_next].next;
                    }
                    m.delete(e_lnext);
                }
                if e == e_next || e == e_next ^ 1 {
                    e_next = m.e[e_next].next;
                }
                m.delete(e);
            }
            e = e_next;
        }
    }

    fn init_priority_q(&mut self) {
        let mut vertex_count = 0;
        let mut v = self.m.v[0].next;
        while v != 0 {
            vertex_count += 1;
            v = self.m.v[v].next;
        }
        vertex_count += 8; // MAX(8, alloc.extraVertices = 0)
        self.pq = PriorityQ::new(vertex_count);
        let mut v = self.m.v[0].next;
        while v != 0 {
            self.m.v[v].pq_handle = self.pq.insert(&self.m.v, v);
            v = self.m.v[v].next;
        }
        self.pq.init(&self.m.v);
    }

    fn remove_degenerate_faces(&mut self) {
        let m = &mut self.m;
        let mut f = m.f[0].next;
        while f != 0 {
            let f_next = m.f[f].next;
            let e = m.f[f].an_edge;
            if m.lnext(m.lnext(e)) == e {
                m.add_winding(m.onext(e), e);
                m.delete(e);
            }
            f = f_next;
        }
    }

    /// tessComputeInterior
    fn compute_interior(&mut self) {
        self.remove_degenerate_edges();
        self.init_priority_q();
        self.init_edge_dict();
        loop {
            let v = self.pq.extract_min(&self.m.v);
            if v == NIL { break }
            loop {
                let v_next = self.pq.minimum(&self.m.v);
                if v_next == NIL || !vert_eq(self.m.p(v_next), self.m.p(v)) { break }
                let v_next = self.pq.extract_min(&self.m.v);
                self.m.splice(self.m.v[v].an_edge, self.m.v[v_next].an_edge);
            }
            self.sweep_event(v);
        }
        self.event = self.m.org(self.up(self.dict[self.dict[0].next].key));
        self.done_edge_dict();
        self.remove_degenerate_faces();
    }

    // tess.c

    fn compute_normal(&self) -> [f32; 3] {
        let m = &self.m;
        let first = m.v[0].next;
        let (mut min_val, mut max_val) = (m.v[first].coords, m.v[first].coords);
        let (mut min_vert, mut max_vert) = ([first; 3], [first; 3]);
        let mut v = first;
        while v != 0 {
            for i in 0..3 {
                let c = m.v[v].coords[i];
                if c < min_val[i] {
                    min_val[i] = c;
                    min_vert[i] = v;
                }
                if c > max_val[i] {
                    max_val[i] = c;
                    max_vert[i] = v;
                }
            }
            v = m.v[v].next;
        }
        let mut i = 0;
        if max_val[1] - min_val[1] > max_val[0] - min_val[0] { i = 1 }
        if max_val[2] - min_val[2] > max_val[i] - min_val[i] { i = 2 }
        if min_val[i] >= max_val[i] {
            return [0.0, 0.0, 1.0];
        }
        let mut norm = [0.0; 3];
        let mut max_len2 = 0.0;
        let (v1, v2) = (m.v[min_vert[i]].coords, m.v[max_vert[i]].coords);
        let d1 = [v1[0] - v2[0], v1[1] - v2[1], v1[2] - v2[2]];
        let mut v = first;
        while v != 0 {
            let c = m.v[v].coords;
            let d2 = [c[0] - v2[0], c[1] - v2[1], c[2] - v2[2]];
            let t = [d1[1] * d2[2] - d1[2] * d2[1], d1[2] * d2[0] - d1[0] * d2[2], d1[0] * d2[1] - d1[1] * d2[0]];
            let t_len2 = t[0] * t[0] + t[1] * t[1] + t[2] * t[2];
            if t_len2 > max_len2 {
                max_len2 = t_len2;
                norm = t;
            }
            v = m.v[v].next;
        }
        if max_len2 <= 0.0 {
            // ShortAxis
            let mut i = 0;
            if abs(d1[1]) < abs(d1[0]) { i = 1 }
            if abs(d1[2]) < abs(d1[i]) { i = 2 }
            norm = [0.0; 3];
            norm[i] = 1.0;
        }
        norm
    }

    /// CheckOrientation: flips t when the positive-winding faces come out clockwise.
    fn check_orientation(&mut self) {
        let m = &mut self.m;
        let mut area = 0.0f32;
        let mut f = m.f[0].next;
        while f != 0 {
            let mut e = m.f[f].an_edge;
            if m.e[e].winding > 0 {
                loop {
                    let (o, d) = (m.po(e), m.pd(e));
                    area += (o.0 - d.0) * (o.1 + d.1);
                    e = m.lnext(e);
                    if e == m.f[f].an_edge { break }
                }
            }
            f = m.f[f].next;
        }
        if area < 0.0 {
            let mut v = m.v[0].next;
            while v != 0 {
                m.v[v].t = -m.v[v].t;
                v = m.v[v].next;
            }
        }
    }

    /// tessProjectPolygon with the normal computed (tessTesselate's normal is NULL).
    fn project_polygon(&mut self) {
        let norm = self.compute_normal();
        // LongAxis
        let mut i = 0;
        if abs(norm[1]) > abs(norm[0]) { i = 1 }
        if abs(norm[2]) > abs(norm[i]) { i = 2 }
        let (mut s_unit, mut t_unit) = ([0.0f32; 3], [0.0f32; 3]);
        s_unit[(i + 1) % 3] = 1.0;
        s_unit[(i + 2) % 3] = 0.0;
        t_unit[(i + 1) % 3] = if norm[i] > 0.0 { -0.0 } else { 0.0 };
        t_unit[(i + 2) % 3] = if norm[i] > 0.0 { 1.0 } else { -1.0 };
        let dot = |c: [f32; 3], u: [f32; 3]| c[0] * u[0] + c[1] * u[1] + c[2] * u[2];
        let mut v = self.m.v[0].next;
        while v != 0 {
            let c = self.m.v[v].coords;
            self.m.v[v].s = dot(c, s_unit);
            self.m.v[v].t = dot(c, t_unit);
            v = self.m.v[v].next;
        }
        self.check_orientation();
        let mut first = true;
        let mut v = self.m.v[0].next;
        while v != 0 {
            let (s, t) = self.m.p(v);
            if first {
                self.bmin = [s, t];
                self.bmax = [s, t];
                first = false;
            } else {
                if s < self.bmin[0] { self.bmin[0] = s }
                if s > self.bmax[0] { self.bmax[0] = s }
                if t < self.bmin[1] { self.bmin[1] = t }
                if t > self.bmax[1] { self.bmax[1] = t }
            }
            v = self.m.v[v].next;
        }
    }

    /// OutputPolymesh for TESS_POLYGONS with vertexSize 2.
    fn output_polymesh(&mut self, poly_size: usize) -> (Vec<[f32; 2]>, Vec<i32>) {
        let m = &mut self.m;
        if poly_size > 3 {
            m.merge_convex_faces(poly_size);
        }
        let mut v = m.v[0].next;
        while v != 0 {
            m.v[v].n = TESS_UNDEF;
            v = m.v[v].next;
        }
        let mut max_vertex_count = 0;
        let mut f = m.f[0].next;
        while f != 0 {
            m.f[f].n = TESS_UNDEF;
            if m.f[f].inside {
                let mut edge = m.f[f].an_edge;
                loop {
                    let v = m.org(edge);
                    if m.v[v].n == TESS_UNDEF {
                        m.v[v].n = max_vertex_count;
                        max_vertex_count += 1;
                    }
                    edge = m.lnext(edge);
                    if edge == m.f[f].an_edge { break }
                }
            }
            f = m.f[f].next;
        }
        let mut vertices = vec![[0.0; 2]; max_vertex_count as usize];
        let mut v = m.v[0].next;
        while v != 0 {
            if m.v[v].n != TESS_UNDEF {
                vertices[m.v[v].n as usize] = [m.v[v].coords[0], m.v[v].coords[1]];
            }
            v = m.v[v].next;
        }
        let mut elements = Vec::new();
        let mut f = m.f[0].next;
        while f != 0 {
            if m.f[f].inside {
                let mut edge = m.f[f].an_edge;
                let mut face_verts = 0;
                loop {
                    elements.push(m.v[m.org(edge)].n);
                    face_verts += 1;
                    edge = m.lnext(edge);
                    if edge == m.f[f].an_edge { break }
                }
                elements.extend(std::iter::repeat(TESS_UNDEF).take(poly_size.saturating_sub(face_verts)));
            }
            f = m.f[f].next;
        }
        (vertices, elements)
    }

    /// tessAddContour with size 2 and reverseContours off.
    fn add_contour(&mut self, points: &[[f32; 2]], vertex_index_counter: &mut i32) {
        let m = &mut self.m;
        let mut e = NIL;
        for p in points {
            if e == NIL {
                e = m.make_edge();
                m.splice(e, e ^ 1);
            } else {
                m.split_edge(e);
                e = m.lnext(e);
            }
            let o = m.org(e);
            m.v[o].coords = [p[0], p[1], 0.0];
            m.v[o].idx = *vertex_index_counter;
            *vertex_index_counter += 1;
            m.e[e].winding = 1;
            m.e[e ^ 1].winding = -1;
        }
    }
}

/// tessNewTess(NULL), tessAddContour(tess, 2, contour, 8, n) for each contour, then tessTesselate(tess, ODD,
/// TESS_POLYGONS, poly_size, 2, NULL): the vertices and the polygons (`poly_size` vertex indices each, padded
/// with TESS_UNDEF), or None when there was nothing to tesselate. The CDT refinement, other winding rules and
/// element types and a caller-given normal are not ported: Unity uses none of them.
pub fn tesselate(contours: &[Vec<[f32; 2]>], poly_size: usize) -> Option<(Vec<[f32; 2]>, Vec<i32>)> {
    if contours.is_empty() {
        return None;
    }
    let mut tess = Tess { m: Mesh::new(), bmin: [0.0; 2], bmax: [0.0; 2], dict: Vec::new(), regions: Vec::new(), pq: PriorityQ::new(1), event: NIL };
    let mut counter = 0;
    for c in contours {
        tess.add_contour(c, &mut counter);
    }
    tess.project_polygon();
    tess.compute_interior();
    tess.m.tessellate_interior();
    Some(tess.output_polymesh(poly_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn polys(r: &(Vec<[f32; 2]>, Vec<i32>), n: usize) -> Vec<Vec<[f32; 2]>> {
        r.1.chunks(n).map(|p| p.iter().filter(|&&i| i != TESS_UNDEF).map(|&i| r.0[i as usize]).collect()).collect()
    }

    fn area(p: &[[f32; 2]]) -> f32 {
        (0..p.len()).map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            a[0] * b[1] - b[0] * a[1]
        }).sum::<f32>() / 2.0
    }

    #[test]
    fn square_is_one_convex_polygon() {
        let r = tesselate(&[vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]], 8).unwrap();
        let p = polys(&r, 8);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].len(), 4);
        assert_eq!(area(&p[0]), 1.0);
    }

    #[test]
    fn concave_and_holed_shapes_keep_their_area() {
        // An L (area 3) and a 4x4 square with a 2x2 hole (area 12), both clockwise and counter-clockwise.
        let l = vec![[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]];
        let outer = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let hole = vec![[1.0, 1.0], [3.0, 1.0], [3.0, 3.0], [1.0, 3.0]];
        for (contours, want) in [(vec![l.clone()], 3.0), (vec![l.into_iter().rev().collect()], 3.0), (vec![outer, hole], 12.0)] {
            let r = tesselate(&contours, 8).unwrap();
            let p = polys(&r, 8);
            // The polygons wind like the contours' positive winding: clockwise input gives clockwise polygons.
            let sign = area(&p[0]).signum();
            assert!(p.iter().all(|p| area(p).signum() == sign), "{p:?}");
            assert_eq!(p.iter().map(|p| area(p).abs()).sum::<f32>(), want, "{p:?}");
        }
    }

    #[test]
    fn self_intersecting_bowtie_splits_at_the_crossing() {
        let r = tesselate(&[vec![[0.0, 0.0], [2.0, 2.0], [2.0, 0.0], [0.0, 2.0]]], 8).unwrap();
        let p = polys(&r, 8);
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(r.0.contains(&[1.0, 1.0]), "{:?}", r.0);
        assert_eq!(p.iter().map(|p| area(p)).sum::<f32>(), 2.0);
    }
}
