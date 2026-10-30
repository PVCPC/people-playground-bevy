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
    Script { class: String, enabled: bool, fields: Value },
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
#[derive(Clone, Debug)]
pub struct Prefab {
    pub objects: Vec<PrefabObject>,
    /// Component or GameObject -> (object index, component index; usize::MAX for the GameObject itself).
    pub index: HashMap<ObjRef, (usize, usize)>,
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
                Some((script, mb, fields)) => Component::Script { class: script.class_name, enabled: mb.enabled, fields },
                None => Component::Other(114),
            },
            c => Component::Other(c),
        })
    }

    /// The GameObject tree rooted at `root` (a GameObject).
    pub fn prefab(&mut self, root: ObjRef) -> Result<Prefab, String> {
        let mut p = Prefab { objects: Vec::new(), index: HashMap::new() };
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
        if std::env::var_os("PPG_LIST_PERSON").is_some() {
            eprintln!("objects+components: {}", p.objects.iter().map(|o| 1 + o.components.len()).sum::<usize>());
            for o in &p.objects {
                for (_, c) in &o.components {
                    match c {
                        Component::Collider2D(c) => eprintln!("collider {}: {:?}", o.name, c.shape),
                        Component::Rigidbody2D(r) => eprintln!("rb {}: {:?}", o.name, r),
                        Component::Joint2D(j) => eprintln!("joint {}: {:?}", o.name, j),
                        _ => {}
                    }
                }
            }
            let mut rbs: Vec<(i64, &str)> = Vec::new();
            let mut cols: Vec<(i64, &str)> = Vec::new();
            for o in &p.objects {
                for (r, c) in &o.components {
                    match c {
                        Component::Rigidbody2D(_) => rbs.push((r.path_id, &o.name)),
                        Component::Collider2D(_) => cols.push((r.path_id, &o.name)),
                        _ => {}
                    }
                }
            }
            let mut gos: Vec<(i64, &str)> = p.objects.iter().map(|o| (o.game_object.path_id, o.name.as_str())).collect();
            gos.sort();
            eprintln!("go order: {gos:?}");
            let mut by_byte: Vec<(u64, &str)> = p.objects.iter().map(|o| (db.files[o.game_object.file].object(o.game_object.path_id).unwrap().byte_start, o.name.as_str())).collect();
            by_byte.sort();
            eprintln!("go byte order: {by_byte:?}");
            let mut ts: Vec<(i64, &str)> = p.objects.iter().map(|o| (o.components[0].0.path_id, o.name.as_str())).collect();
            ts.sort();
            eprintln!("transform order: {ts:?}");
            // PreloadData (150) m_Assets order of each Rigidbody2D.
            for file in 0..db.files.len() {
                for o in db.files[file].objects.iter().filter(|o| o.class_id == 150) {
                    let d = db.files[file].data(o);
                    let mut r = Reader::new(d);
                    let _name = r.string().unwrap();
                    let n = r.i32().unwrap();
                    let assets: Vec<PPtr> = (0..n).map(|_| PPtr::read(&mut r).unwrap()).collect();
                    let mut pos: Vec<(usize, &str)> = Vec::new();
                    for ob in &p.objects {
                        for (cr, c) in &ob.components {
                            if matches!(c, Component::Rigidbody2D(_)) {
                                if let Some(i) = assets.iter().position(|a| db.resolve(file, *a) == Some(*cr)) {
                                    pos.push((i, &ob.name));
                                }
                            }
                        }
                    }
                    pos.sort();
                    eprintln!("preload {} #{}: {} assets, rb order {pos:?}", file, o.path_id, n);
                }
            }
            rbs.sort();
            cols.sort();
            eprintln!("rb order: {rbs:?}");
            eprintln!("collider order: {cols:?}");
            eprintln!("slots: {}", p.objects.iter().map(|o| format!("{}={}", o.name, 1 + o.components.len())).collect::<Vec<_>>().join(" "));
            for o in &p.objects {
                let comps: Vec<String> = o
                    .components
                    .iter()
                    .map(|(_, c)| match c {
                        Component::Script { class, enabled, .. } => format!("{class}{}", if *enabled { "" } else { "(off)" }),
                        Component::Other(c) => format!("#{c}"),
                        c => format!("{:?}", std::mem::discriminant(c)).replace("Discriminant", ""),
                    })
                    .collect();
                eprintln!("{}{} {}: {}", "  ".repeat(depth(&p, o)), o.name, if o.active { "" } else { "(inactive)" }, comps.join(", "));
            }
        }
        assert_eq!(limbs.len(), 14, "{:?}", limbs.iter().map(|l| &l.name).collect::<Vec<_>>());
        for l in limbs {
            assert!(l.components.iter().any(|(_, c)| matches!(c, Component::Rigidbody2D(_))), "{}", l.name);
            assert!(l.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_))), "{}", l.name);
        }
    }
}
