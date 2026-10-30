//! The installed game's object graph: every SerializedFile of the build, PPtrs resolved across them, scripts read
//! with their classes, and prefabs as GameObject trees.

use super::classes::{GameObject, MonoBehaviour, MonoScript, PPtr, Transform};
use super::components::{Collider2D, Joint2D, Rigidbody2D, SpriteRenderer};
use super::mono::{Managed, Value};
use super::serialized::{Reader, SerializedFile};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// An object anywhere in the build: (file index, path id).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjRef {
    pub file: usize,
    pub path_id: i64,
}

pub struct AssetDb {
    pub data_dir: PathBuf,
    pub files: Vec<SerializedFile>,
    names: HashMap<String, usize>,
    pub managed: Managed,
    scripts: HashMap<ObjRef, MonoScript>,
}

#[derive(Clone, Debug)]
pub enum Component {
    Transform(Transform),
    Rigidbody2D(Rigidbody2D),
    Collider2D(Collider2D),
    Joint2D(Joint2D),
    SpriteRenderer(SpriteRenderer),
    /// A MonoBehaviour: its script class, MonoScript.m_ExecutionOrder, Behaviour.m_Enabled and fields.
    Script { class: String, execution_order: i32, enabled: bool, fields: Value },
    /// A component class the rewrite does not read yet (audio, particles, UI...).
    Other(i32),
}

#[derive(Clone, Debug)]
pub struct PrefabObject {
    pub game_object: ObjRef,
    pub name: String,
    pub layer: u32,
    pub tag: u16,
    pub active: bool,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub components: Vec<(ObjRef, Component)>,
}

/// A GameObject hierarchy, root first, children in Transform order (depth first).
#[derive(Clone, Debug, Default)]
pub struct Prefab {
    pub objects: Vec<PrefabObject>,
    /// Component or GameObject -> (object index, component index; usize::MAX for the GameObject itself).
    pub index: HashMap<ObjRef, (usize, usize)>,
    /// The PhysicsMaterial2D a Collider2D or Rigidbody2D component points at, by (object, component): (friction,
    /// bounciness). A material that does not resolve reads as null and has no entry.
    pub materials: HashMap<(usize, usize), (f32, f32)>,
}

impl AssetDb {
    pub fn open(game: &Path) -> Result<Self, String> {
        let data_dir = game.join("People Playground_Data");
        let mut names: Vec<String> = std::fs::read_dir(&data_dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| (n.ends_with(".assets") || n.starts_with("level") || n == "globalgamemanagers") && !n.contains('.') || n.ends_with(".assets"))
            .collect();
        names.sort();
        let mut files = Vec::new();
        let mut index = HashMap::new();
        for n in names {
            index.insert(n.clone(), files.len());
            files.push(SerializedFile::open(&data_dir.join(&n))?);
        }
        Ok(Self { data_dir, files, names: index, managed: Managed::new(game), scripts: HashMap::new() })
    }

    pub fn file_index(&self, name: &str) -> Option<usize> {
        self.names.get(name).copied()
    }

    /// A PPtr written in `from`, resolved; None for null or targets outside the build (Unity's builtin resources).
    pub fn resolve(&self, from: usize, p: PPtr) -> Option<ObjRef> {
        if p.is_null() {
            return None;
        }
        let file = if p.file == 0 {
            from
        } else {
            let ext = self.files[from].externals.get(p.file as usize - 1)?;
            self.file_index(ext.path.rsplit('/').next()?)?
        };
        self.files[file].object(p.path_id)?;
        Some(ObjRef { file, path_id: p.path_id })
    }

    pub fn class(&self, r: ObjRef) -> i32 {
        self.files[r.file].object(r.path_id).map_or(-1, |o| o.class_id)
    }

    /// The order of the persistent instance ids Unity gives the Resources objects (every Instantiate-able prefab
    /// and its dependencies), as ids 2, 4, ... in that order. They are allocated while globalgamemanagers'
    /// ResourceManager is read at startup: each PPtr it transfers (m_Container's targets, then each
    /// m_DependentAssets entry's object and dependencies) takes the next id on its first appearance
    /// (Remapper::GetOrGenerateInstanceID). The managers read before it (MonoManager's scripts, GraphicsSettings'
    /// shaders, ...) already gave a few of these targets lower ids, but none of them is a GameObject or a
    /// component, so the order among a prefab's own objects (all `instantiate_native` needs) is exact.
    pub fn resource_load_order(&self) -> Result<HashMap<ObjRef, i32>, String> {
        let file = self.file_index("globalgamemanagers").ok_or("no globalgamemanagers")?;
        let manager = self.files[file].objects.iter().find(|o| o.class_id == 147).ok_or("no ResourceManager")?;
        let r = &mut Reader::new(self.data(ObjRef { file, path_id: manager.path_id }));
        let mut order = HashMap::new();
        let mut take = |p: PPtr| {
            if let Some(o) = self.resolve(file, p) {
                let next = 2 * order.len() as i32 + 2;
                order.entry(o).or_insert(next);
            }
        };
        for _ in 0..r.i32()? {
            r.string()?;
            take(PPtr::read(r)?);
        }
        for _ in 0..r.i32()? {
            take(PPtr::read(r)?);
            for _ in 0..r.i32()? {
                take(PPtr::read(r)?);
            }
        }
        Ok(order)
    }

    /// Resources.Load(path) of an object of `class_id`: the ResourceManager's m_Container entry with that path
    /// (Unity lowercases both sides) and class.
    pub fn resource(&self, path: &str, class_id: i32) -> Result<Option<ObjRef>, String> {
        let file = self.file_index("globalgamemanagers").ok_or("no globalgamemanagers")?;
        let manager = self.files[file].objects.iter().find(|o| o.class_id == 147).ok_or("no ResourceManager")?;
        let r = &mut Reader::new(self.data(ObjRef { file, path_id: manager.path_id }));
        let want = path.to_lowercase();
        for _ in 0..r.i32()? {
            let name = r.string()?;
            let target = PPtr::read(r)?;
            if name == want {
                if let Some(t) = self.resolve(file, target).filter(|&t| self.class(t) == class_id) {
                    return Ok(Some(t));
                }
            }
        }
        Ok(None)
    }

    pub fn data(&self, r: ObjRef) -> &[u8] {
        let f = &self.files[r.file];
        f.data(f.object(r.path_id).expect("object"))
    }

    pub fn game_object(&self, r: ObjRef) -> Result<GameObject, String> {
        GameObject::read(self.data(r))
    }

    /// A MonoBehaviour's script class, or None for a missing script.
    pub fn script(&mut self, r: ObjRef) -> Result<Option<MonoScript>, String> {
        let mb = MonoBehaviour::read(self.data(r))?;
        let Some(s) = self.resolve(r.file, mb.script) else { return Ok(None) };
        if !self.scripts.contains_key(&s) {
            let script = MonoScript::read(self.data(s))?;
            self.scripts.insert(s, script);
        }
        Ok(self.scripts.get(&s).cloned())
    }

    /// A MonoBehaviour's header and fields; None for a missing script.
    pub fn mono(&mut self, r: ObjRef) -> Result<Option<(MonoScript, MonoBehaviour, Value)>, String> {
        let Some(script) = self.script(r)? else { return Ok(None) };
        let asm = self.managed.assembly(&script.assembly)?;
        if asm.type_def(&script.namespace, &script.class_name).is_none() {
            return Ok(None);
        }
        let f = &self.files[r.file];
        let data = f.data(f.object(r.path_id).unwrap());
        let mb = MonoBehaviour::read(data)?;
        let mut reader = Reader::new(data);
        reader.pos = mb.fields;
        let v = self.managed.read_script(&script.assembly, &script.namespace, &script.class_name, &mut reader)?;
        if reader.pos != data.len() {
            return Err(format!("{}: read {} of {} bytes", script.class_name, reader.pos, data.len()));
        }
        Ok(Some((script, mb, v)))
    }

    /// Every MonoBehaviour / ScriptableObject whose script class is `class`.
    pub fn find_scripts(&mut self, class: &str) -> Result<Vec<ObjRef>, String> {
        let mut out = Vec::new();
        for file in 0..self.files.len() {
            let ids: Vec<i64> = self.files[file].objects.iter().filter(|o| o.class_id == 114).map(|o| o.path_id).collect();
            for path_id in ids {
                let r = ObjRef { file, path_id };
                if self.script(r)?.is_some_and(|s| s.class_name == class) {
                    out.push(r);
                }
            }
        }
        Ok(out)
    }

    fn component(&mut self, r: ObjRef) -> Result<Component, String> {
        let d = self.data(r);
        Ok(match self.class(r) {
            // RectTransform (224) is a Transform with its UI layout fields after.
            4 | 224 => Component::Transform(Transform::read(d)?),
            50 => Component::Rigidbody2D(Rigidbody2D::read(d)?),
            212 => Component::SpriteRenderer(SpriteRenderer::read(d)?),
            c if Collider2D::CLASSES.contains(&c) => Component::Collider2D(Collider2D::read(c, d)?),
            c if Joint2D::CLASSES.contains(&c) => Component::Joint2D(Joint2D::read(c, d)?),
            114 => match self.mono(r)? {
                Some((script, mb, fields)) => Component::Script { class: script.class_name, execution_order: script.execution_order, enabled: mb.enabled, fields },
                None => Component::Other(114),
            },
            c => Component::Other(c),
        })
    }

    /// The GameObject tree rooted at `root` (a GameObject).
    pub fn prefab(&mut self, root: ObjRef) -> Result<Prefab, String> {
        let mut p = Prefab { objects: Vec::new(), index: HashMap::new(), materials: HashMap::new() };
        self.add_object(&mut p, root, None)?;
        Ok(p)
    }

    fn add_object(&mut self, p: &mut Prefab, go: ObjRef, parent: Option<usize>) -> Result<usize, String> {
        let g = self.game_object(go)?;
        let me = p.objects.len();
        p.index.insert(go, (me, usize::MAX));
        p.objects.push(PrefabObject {
            game_object: go,
            name: g.name.clone(),
            layer: g.layer,
            tag: g.tag,
            active: g.active,
            parent,
            children: Vec::new(),
            components: Vec::new(),
        });
        let mut children = Vec::new();
        for c in &g.components {
            let r = self.resolve(go.file, *c).ok_or_else(|| format!("{}: unresolved component", g.name))?;
            let comp = self.component(r).map_err(|e| format!("{}: {e}", g.name))?;
            if let Component::Transform(t) = &comp {
                children = t.children.clone();
            }
            let material = match &comp {
                Component::Collider2D(c) => c.material,
                Component::Rigidbody2D(r) => r.material,
                _ => PPtr::default(),
            };
            if let Some(m) = self.resolve(go.file, material) {
                let m = super::components::PhysicsMaterial2D::read(self.data(m))?;
                p.materials.insert((me, p.objects[me].components.len()), (m.friction, m.bounciness));
            }
            p.index.insert(r, (me, p.objects[me].components.len()));
            p.objects[me].components.push((r, comp));
        }
        for c in children {
            let t = self.resolve(go.file, c).ok_or("unresolved child transform")?;
            let child_go = self.resolve(t.file, Transform::read(self.data(t))?.game_object).ok_or("child without GameObject")?;
            let idx = self.add_object(p, child_go, Some(me))?;
            p.objects[me].children.push(idx);
        }
        Ok(me)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn depth(p: &Prefab, o: &PrefabObject) -> usize {
        o.parent.map_or(0, |i| 1 + depth(p, &p.objects[i]))
    }

    /// Prints every CatalogData item's asset name, one per line after "ITEM " (for the catalog sweep runner).
    #[test]
    #[ignore]
    fn catalog_item_names() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let mut db = AssetDb::open(&game).unwrap();
        let catalog = db.find_scripts("CatalogData").unwrap()[0];
        let (_, _, catalog_data) = db.mono(catalog).unwrap().unwrap();
        for item in catalog_data.get("Items").and_then(Value::array).unwrap().to_vec() {
            let asset = db.resolve(catalog.file, item.pptr().unwrap()).unwrap();
            println!("ITEM {}", db.mono(asset).unwrap().unwrap().1.name);
        }
    }

    /// Lists the catalog prefabs' Rigidbody2Ds under another Rigidbody2D: child (type), nearest body above (type),
    /// the child's scripts and joints. Run with --ignored --nocapture.
    #[test]
    #[ignore]
    fn catalog_nested_bodies() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let mut db = AssetDb::open(&game).unwrap();
        let catalog = db.find_scripts("CatalogData").unwrap()[0];
        let (_, _, catalog_data) = db.mono(catalog).unwrap().unwrap();
        for item in catalog_data.get("Items").and_then(Value::array).unwrap().to_vec() {
            let asset = db.resolve(catalog.file, item.pptr().unwrap()).unwrap();
            let (_, _, spawnable) = db.mono(asset).unwrap().unwrap();
            let Some(prefab) = spawnable.get("Prefab").and_then(Value::pptr).and_then(|p| db.resolve(asset.file, p)) else { continue };
            let p = db.prefab(prefab).unwrap();
            let body = |o: usize| p.objects[o].components.iter().find_map(|(_, c)| match c { Component::Rigidbody2D(rb) => Some(rb.body_type), _ => None });
            for o in 0..p.objects.len() {
                let Some(t) = body(o) else { continue };
                let mut up = p.objects[o].parent;
                while let Some(u) = up {
                    if let Some(tu) = body(u) {
                        let what: Vec<String> = p.objects[o].components.iter().filter_map(|(_, c)| match c {
                            Component::Script { class, .. } => Some(class.clone()),
                            Component::Joint2D(j) => Some(format!("{:?}", std::mem::discriminant(&j.kind)).chars().take(0).collect::<String>() + "Joint"),
                            _ => None,
                        }).collect();
                        println!("NEST {} | {} ({t}) under {} ({tu}) | {}", p.objects[0].name, p.objects[o].name, p.objects[u].name, what.join(","));
                        break;
                    }
                    up = p.objects[u].parent;
                }
            }
        }
    }

    /// The game's catalog (CatalogData.Items) resolves, and every spawnable prefab's tree reads completely.
    #[test]
    fn catalog_prefabs_load() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let mut db = AssetDb::open(&game).unwrap();
        let catalogs = db.find_scripts("CatalogData").unwrap();
        assert_eq!(catalogs.len(), 1, "one CatalogData");
        let (_, _, catalog) = db.mono(catalogs[0]).unwrap().unwrap();
        let items = catalog.get("Items").and_then(Value::array).unwrap().to_vec();
        assert!(items.len() >= 284, "{} items", items.len());
        let mut other: BTreeMap<i32, usize> = BTreeMap::new();
        let mut names = Vec::new();
        for item in &items {
            let asset = db.resolve(catalogs[0].file, item.pptr().unwrap()).expect("SpawnableAsset");
            let (_, _, spawnable) = db.mono(asset).unwrap().unwrap();
            let Some(prefab) = spawnable.get("Prefab").and_then(Value::pptr).and_then(|p| db.resolve(asset.file, p)) else {
                continue;
            };
            let p = db.prefab(prefab).unwrap();
            names.push((p.objects[0].name.clone(), prefab));
            for o in &p.objects {
                for (_, c) in &o.components {
                    if let Component::Other(c) = c {
                        *other.entry(*c).or_default() += 1;
                    }
                }
            }
        }
        eprintln!("{} prefabs; unread component classes: {other:?}", names.len());
        assert!(names.iter().any(|(n, _)| n == "Knife"), "no Knife prefab in the catalog");
        // Person's limbs: each has a LimbBehaviour, a Rigidbody2D and a collider.
        let person = names.iter().find(|(n, _)| n == "Person").expect("no Person prefab in the catalog").1;
        let p = db.prefab(person).unwrap();
        let limbs: Vec<&PrefabObject> = p
            .objects
            .iter()
            .filter(|o| o.components.iter().any(|(_, c)| matches!(c, Component::Script { class, .. } if class == "LimbBehaviour")))
            .collect();
        assert_eq!(limbs.len(), 14, "{:?}", limbs.iter().map(|l| &l.name).collect::<Vec<_>>());
        for l in limbs {
            assert!(l.components.iter().any(|(_, c)| matches!(c, Component::Rigidbody2D(_))), "{}", l.name);
            assert!(l.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_))), "{}", l.name);
        }
    }
}
