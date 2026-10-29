//! b2DynamicTree and b2BroadPhase as UnityPlayer.dll compiles them. Node indices are proxy ids and decide pair
//! (and so contact) order, so the free list, growth and rotations follow the native code exactly.

use super::math::Vec2;

pub const NULL: i32 = -1;
/// b2_aabbExtension.
const AABB_EXTENSION: f32 = f32::from_bits(0x3dcc_cccd);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aabb {
    pub lower: Vec2,
    pub upper: Vec2,
}

impl Aabb {
    pub fn combine(a: &Aabb, b: &Aabb) -> Aabb {
        let lo = |x: f32, y: f32| if y <= x { y } else { x };
        let hi = |x: f32, y: f32| if x <= y { y } else { x };
        Aabb { lower: Vec2::new(lo(a.lower.x, b.lower.x), lo(a.lower.y, b.lower.y)), upper: Vec2::new(hi(a.upper.x, b.upper.x), hi(a.upper.y, b.upper.y)) }
    }

    fn perimeter(&self) -> f32 {
        let p = (self.upper.y - self.lower.y) + (self.upper.x - self.lower.x);
        p + p
    }

    fn contains(&self, aabb: &Aabb) -> bool {
        !(aabb.lower.x < self.lower.x || aabb.lower.y < self.lower.y || self.upper.x < aabb.upper.x || self.upper.y < aabb.upper.y)
    }

    fn overlaps(&self, node: &Aabb) -> bool {
        self.lower.x - node.upper.x <= 0.0 && self.lower.y - node.upper.y <= 0.0 && node.lower.x - self.upper.x <= 0.0 && node.lower.y - self.upper.y <= 0.0
    }
}

/// b2TreeNode; `parent` doubles as the free-list link. Freed nodes keep their stale box and user data.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TreeNode {
    pub aabb: Aabb,
    pub user_data: usize,
    pub parent: i32,
    pub child1: i32,
    pub child2: i32,
    pub height: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DynamicTree {
    pub root: i32,
    pub nodes: Vec<TreeNode>,
    pub node_count: i32,
    pub free_list: i32,
    pub insertion_count: i32,
}

impl DynamicTree {
    pub fn new(capacity: usize) -> Self {
        let mut tree = Self { root: NULL, nodes: vec![TreeNode::default(); capacity], node_count: 0, free_list: 0, insertion_count: 0 };
        tree.link_free(0);
        tree
    }

    /// Chains nodes[from..] into the free list in ascending order.
    fn link_free(&mut self, from: usize) {
        let last = self.nodes.len() - 1;
        for i in from..last {
            self.nodes[i].parent = i as i32 + 1;
            self.nodes[i].height = -1;
        }
        self.nodes[last].parent = NULL;
        self.nodes[last].height = -1;
        self.free_list = from as i32;
    }

    /// AllocateNode (180baab20): doubles capacity when the free list runs out.
    fn allocate_node(&mut self) -> i32 {
        if self.free_list == NULL {
            let count = self.nodes.len();
            self.nodes.resize(count * 2, TreeNode::default());
            self.link_free(self.node_count as usize);
        }
        let id = self.free_list;
        let node = &mut self.nodes[id as usize];
        self.free_list = node.parent;
        node.parent = NULL;
        node.child1 = NULL;
        node.child2 = NULL;
        node.height = 0;
        node.user_data = 0;
        self.node_count += 1;
        id
    }

    fn free_node(&mut self, id: i32) {
        self.nodes[id as usize].parent = self.free_list;
        self.nodes[id as usize].height = -1;
        self.node_count -= 1;
        self.free_list = id;
    }

    fn node(&self, id: i32) -> &TreeNode {
        &self.nodes[id as usize]
    }

    fn node_mut(&mut self, id: i32) -> &mut TreeNode {
        &mut self.nodes[id as usize]
    }

    /// Re-fits the boxes and heights from `index` to the root, balancing on the way.
    fn refit(&mut self, mut index: i32) {
        while index != NULL {
            index = self.balance(index);
            let (child1, child2) = (self.node(index).child1, self.node(index).child2);
            let height = 1 + self.node(child1).height.max(self.node(child2).height);
            let aabb = Aabb::combine(&self.node(child1).aabb, &self.node(child2).aabb);
            let node = self.node_mut(index);
            node.height = height;
            node.aabb = aabb;
            index = node.parent;
        }
    }

    /// InsertLeaf (180bae090): the surface-area-heuristic descent.
    fn insert_leaf(&mut self, leaf: i32) {
        self.insertion_count += 1;
        if self.root == NULL {
            self.root = leaf;
            self.node_mut(leaf).parent = NULL;
            return;
        }
        let leaf_aabb = self.node(leaf).aabb;
        let mut index = self.root;
        while self.node(index).child1 != NULL {
            let node = self.node(index);
            let (child1, child2) = (node.child1, node.child2);
            let area = node.aabb.perimeter();
            let combined_area = Aabb::combine(&node.aabb, &leaf_aabb).perimeter();
            let cost = combined_area + combined_area;
            let inheritance = combined_area - area;
            let inheritance = inheritance + inheritance;
            let descend = |child: i32| {
                let c = self.node(child);
                let new_area = Aabb::combine(&leaf_aabb, &c.aabb).perimeter();
                inheritance + if c.child1 == NULL { new_area } else { new_area - c.aabb.perimeter() }
            };
            let (cost1, cost2) = (descend(child1), descend(child2));
            if cost < cost1 && cost < cost2 {
                break;
            }
            index = if cost1 < cost2 { child1 } else { child2 };
        }
        let sibling = index;
        let old_parent = self.node(sibling).parent;
        let new_parent = self.allocate_node();
        let aabb = Aabb::combine(&leaf_aabb, &self.node(sibling).aabb);
        let height = self.node(sibling).height + 1;
        *self.node_mut(new_parent) = TreeNode { aabb, user_data: 0, parent: old_parent, child1: sibling, child2: leaf, height };
        if old_parent == NULL {
            self.root = new_parent;
        } else if self.node(old_parent).child1 == sibling {
            self.node_mut(old_parent).child1 = new_parent;
        } else {
            self.node_mut(old_parent).child2 = new_parent;
        }
        self.node_mut(sibling).parent = new_parent;
        self.node_mut(leaf).parent = new_parent;
        self.refit(new_parent);
    }

    /// RemoveLeaf (180bae990).
    fn remove_leaf(&mut self, leaf: i32) {
        if leaf == self.root {
            self.root = NULL;
            return;
        }
        let parent = self.node(leaf).parent;
        let grand_parent = self.node(parent).parent;
        let sibling = if self.node(parent).child1 == leaf { self.node(parent).child2 } else { self.node(parent).child1 };
        if grand_parent == NULL {
            self.root = sibling;
            self.node_mut(sibling).parent = NULL;
            self.free_node(parent);
            return;
        }
        if self.node(grand_parent).child1 == parent {
            self.node_mut(grand_parent).child1 = sibling;
        } else {
            self.node_mut(grand_parent).child2 = sibling;
        }
        self.node_mut(sibling).parent = grand_parent;
        self.free_node(parent);
        self.refit(grand_parent);
    }

    /// Balance (180baac50): one AVL-style rotation when the children's heights differ by more than one.
    fn balance(&mut self, a: i32) -> i32 {
        if self.node(a).child1 == NULL || self.node(a).height < 2 {
            return a;
        }
        let (b, c) = (self.node(a).child1, self.node(a).child2);
        let balance = self.node(c).height - self.node(b).height;
        // Rotate `up` (C when right-heavy, B when left-heavy) above A; `stay` is A's other child.
        let (up, stay, up_is_child2) = if balance > 1 {
            (c, b, true)
        } else if balance < -1 {
            (b, c, false)
        } else {
            return a;
        };
        let (f, g) = (self.node(up).child1, self.node(up).child2);
        let a_parent = self.node(a).parent;
        self.node_mut(up).child1 = a;
        self.node_mut(up).parent = a_parent;
        self.node_mut(a).parent = up;
        if a_parent == NULL {
            self.root = up;
        } else if self.node(a_parent).child1 == a {
            self.node_mut(a_parent).child1 = up;
        } else {
            self.node_mut(a_parent).child2 = up;
        }
        let (keep, give) = if self.node(g).height < self.node(f).height { (f, g) } else { (g, f) };
        self.node_mut(up).child2 = keep;
        if up_is_child2 {
            self.node_mut(a).child2 = give;
        } else {
            self.node_mut(a).child1 = give;
        }
        self.node_mut(give).parent = a;
        let a_aabb = Aabb::combine(&self.node(stay).aabb, &self.node(give).aabb);
        let a_height = 1 + self.node(stay).height.max(self.node(give).height);
        let up_aabb = Aabb::combine(&a_aabb, &self.node(keep).aabb);
        let up_height = 1 + a_height.max(self.node(keep).height);
        self.node_mut(a).aabb = a_aabb;
        self.node_mut(a).height = a_height;
        self.node_mut(up).aabb = up_aabb;
        self.node_mut(up).height = up_height;
        up
    }

    /// Query<T> (180bb72b0): leaves overlapping `aabb`, child2 subtrees first; stops when `callback` returns false.
    pub fn query(&self, aabb: &Aabb, mut callback: impl FnMut(i32) -> bool) {
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            if id == NULL {
                continue;
            }
            let node = self.node(id);
            if aabb.overlaps(&node.aabb) {
                if node.child1 == NULL {
                    if !callback(id) {
                        return;
                    }
                } else {
                    stack.push(node.child1);
                    stack.push(node.child2);
                }
            }
        }
    }

    pub fn fat_aabb(&self, id: i32) -> &Aabb {
        &self.node(id).aabb
    }

    pub fn user_data(&self, id: i32) -> usize {
        self.node(id).user_data
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BroadPhase {
    pub tree: DynamicTree,
    pub proxy_count: i32,
    pub move_buffer: Vec<i32>,
    pairs: Vec<(i32, i32)>,
}

impl Default for BroadPhase {
    fn default() -> Self {
        Self::with_capacity(16)
    }
}

impl BroadPhase {
    pub fn with_capacity(capacity: usize) -> Self {
        Self { tree: DynamicTree::new(capacity), proxy_count: 0, move_buffer: Vec::new(), pairs: Vec::new() }
    }

    /// CreateProxy (180bac6e0).
    pub fn create_proxy(&mut self, aabb: &Aabb, user_data: usize) -> i32 {
        let id = self.tree.allocate_node();
        let e = AABB_EXTENSION;
        let node = self.tree.node_mut(id);
        node.aabb = Aabb { lower: Vec2::new(aabb.lower.x - e, aabb.lower.y - e), upper: Vec2::new(aabb.upper.x + e, aabb.upper.y + e) };
        node.user_data = user_data;
        node.height = 0;
        self.tree.insert_leaf(id);
        self.proxy_count += 1;
        self.move_buffer.push(id);
        id
    }

    /// DestroyProxy (180bac7d0): Unity swap-removes the id from the move buffer.
    pub fn destroy_proxy(&mut self, id: i32) {
        let mut i = 0;
        while i < self.move_buffer.len() {
            if self.move_buffer[i] == id {
                self.move_buffer.swap_remove(i);
            } else {
                i += 1;
            }
        }
        self.proxy_count -= 1;
        self.tree.remove_leaf(id);
        self.tree.free_node(id);
    }

    /// MoveProxy (180bae4a0): re-inserts with a box fattened by the extension and twice the displacement.
    pub fn move_proxy(&mut self, id: i32, aabb: &Aabb, displacement: Vec2) {
        if self.tree.node(id).aabb.contains(aabb) {
            return;
        }
        self.tree.remove_leaf(id);
        let e = AABB_EXTENSION;
        let mut fat = Aabb { lower: Vec2::new(aabb.lower.x - e, aabb.lower.y - e), upper: Vec2::new(aabb.upper.x + e, aabb.upper.y + e) };
        let d = Vec2::new(displacement.x + displacement.x, displacement.y + displacement.y);
        if 0.0 <= d.x { fat.upper.x += d.x } else { fat.lower.x += d.x }
        if 0.0 <= d.y { fat.upper.y += d.y } else { fat.lower.y += d.y }
        self.tree.node_mut(id).aabb = fat;
        self.tree.insert_leaf(id);
        self.move_buffer.push(id);
    }

    pub fn touch_proxy(&mut self, id: i32) {
        self.move_buffer.push(id);
    }

    /// The pairs Query<b2BroadPhase> + QueryCallback (180bae6b0) append for one moved proxy, as (min, max) ids.
    pub fn query_pairs(&self, query_id: i32, pairs: &mut Vec<(i32, i32)>) {
        self.tree.query(self.tree.fat_aabb(query_id), |id| {
            if id != query_id {
                pairs.push((id.min(query_id), id.max(query_id)));
            }
            true
        });
    }

    /// UpdatePairs (180bb7af0): new overlaps of moved proxies, sorted by (idA, idB) and de-duplicated, as user data.
    pub fn update_pairs(&mut self, mut add_pair: impl FnMut(usize, usize)) {
        let mut pairs = std::mem::take(&mut self.pairs);
        pairs.clear();
        for &id in &self.move_buffer {
            if id != NULL {
                self.query_pairs(id, &mut pairs);
            }
        }
        self.move_buffer.clear();
        pairs.sort_unstable();
        pairs.dedup();
        for &(a, b) in &pairs {
            add_pair(self.tree.user_data(a), self.tree.user_data(b));
        }
        self.pairs = pairs;
    }
}
