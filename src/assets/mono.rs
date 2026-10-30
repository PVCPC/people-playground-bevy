//! Unity's managed serialization (2020.3): a MonoBehaviour's / ScriptableObject's field data, laid out from the
//! script class in the game's assemblies, since player builds carry no type trees.

use super::classes::PPtr;
use super::clr::{self, Assembly, Token};
use super::serialized::Reader;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc as Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f32),
    Double(f64),
    Str(String),
    PPtr(PPtr),
    Array(Vec<Value>),
    Struct(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Struct(fields) => fields.iter().find(|(n, _)| n == name).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn f32(&self) -> Option<f32> {
        match *self {
            Value::Float(v) => Some(v),
            _ => None,
        }
    }
    pub fn i64(&self) -> Option<i64> {
        match *self {
            Value::Int(v) => Some(v),
            Value::UInt(v) => i64::try_from(v).ok(),
            _ => None,
        }
    }
    pub fn bool(&self) -> Option<bool> {
        match *self {
            Value::Bool(v) => Some(v),
            _ => None,
        }
    }
    pub fn pptr(&self) -> Option<PPtr> {
        match *self {
            Value::PPtr(v) => Some(v),
            _ => None,
        }
    }
    pub fn array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(v) => Some(v),
            _ => None,
        }
    }
}

/// A field type, resolved across assemblies.
#[derive(Clone)]
enum Ty {
    /// ELEMENT_TYPE_BOOLEAN .. ELEMENT_TYPE_R8.
    Prim(u8),
    String,
    Def(Rc<Assembly>, u32, Vec<Ty>),
    Array(Box<Ty>),
    /// A generic parameter of the enclosing type.
    Var(u32),
    /// Something the serializer never writes (object, pointers, method generics).
    Never,
}

/// How Unity writes a resolved type.
enum Kind {
    Prim(u8),
    String,
    PPtr,
    Native(&'static str),
    Array(Ty),
    Class(Rc<Assembly>, u32, Vec<Ty>),
}

pub struct Managed {
    dir: PathBuf,
    assemblies: HashMap<String, Rc<Assembly>>,
}

const ELEMENT_BOOL: u8 = 0x02;
const ELEMENT_R8: u8 = 0x0d;

impl Managed {
    pub fn new(game: &std::path::Path) -> Self {
        Self { dir: game.join("People Playground_Data/Managed"), assemblies: HashMap::new() }
    }

    pub fn assembly(&mut self, name: &str) -> Result<Rc<Assembly>, String> {
        let name = name.trim_end_matches(".dll");
        if let Some(a) = self.assemblies.get(name) {
            return Ok(a.clone());
        }
        let a = Rc::new(Assembly::open(&self.dir.join(format!("{name}.dll")))?);
        self.assemblies.insert(name.to_string(), a.clone());
        Ok(a)
    }

    /// A type by assembly and full name, following type forwarders.
    fn find(&mut self, assembly: &str, namespace: &str, name: &str) -> Result<(Rc<Assembly>, u32), String> {
        let a = self.assembly(assembly)?;
        if let Some(t) = a.type_def(namespace, name) {
            return Ok((a, t));
        }
        match a.forwarded(namespace, name) {
            Some(to) => self.find(&to, namespace, name),
            None => Err(format!("{namespace}.{name} not in {assembly}")),
        }
    }

    fn resolve(&mut self, a: &Rc<Assembly>, t: Token) -> Result<(Rc<Assembly>, u32), String> {
        match t.0 {
            clr::TYPE_DEF => Ok((a.clone(), t.1)),
            clr::TYPE_REF => {
                let (namespace, name) = a.type_name(t).ok_or("bad TypeRef")?;
                match a.type_ref_assembly(t.1) {
                    Some(asm) => self.find(&asm, &namespace, &name),
                    None => a.type_def(&namespace, &name).map(|r| (a.clone(), r)).ok_or_else(|| format!("{namespace}.{name}")),
                }
            }
            _ => Err(format!("unresolvable token {t:?}")),
        }
    }

    /// One type from a signature blob, generic parameters substituted from `args`.
    fn sig_type(&mut self, a: &Rc<Assembly>, b: &mut &[u8], args: &[Ty]) -> Result<Ty, String> {
        let e = take(b)?;
        Ok(match e {
            ELEMENT_BOOL..=ELEMENT_R8 => Ty::Prim(e),
            0x0e => Ty::String,
            0x11 | 0x12 => {
                let (r, a2) = self.resolve(a, Assembly::type_def_or_ref(compressed(b)?))?;
                Ty::Def(r, a2, Vec::new())
            }
            0x13 => {
                let n = compressed(b)?;
                args.get(n as usize).cloned().unwrap_or(Ty::Var(n))
            }
            0x15 => {
                take(b)?;
                let (r, t) = self.resolve(a, Assembly::type_def_or_ref(compressed(b)?))?;
                let n = compressed(b)?;
                let mut targs = Vec::new();
                for _ in 0..n {
                    targs.push(self.sig_type(a, b, args)?);
                }
                Ty::Def(r, t, targs)
            }
            0x1d => Ty::Array(Box::new(self.sig_type(a, b, args)?)),
            0x1f | 0x20 => {
                compressed(b)?;
                self.sig_type(a, b, args)?
            }
            0x14 => {
                // A multi-dimensional array: never serialized; skip its shape.
                self.sig_type(a, b, args)?;
                let _rank = compressed(b)?;
                for _ in 0..compressed(b)? {
                    compressed(b)?;
                }
                for _ in 0..compressed(b)? {
                    compressed(b)?;
                }
                Ty::Never
            }
            0x0f | 0x10 => {
                self.sig_type(a, b, args)?;
                Ty::Never
            }
            0x1e => {
                compressed(b)?;
                Ty::Never
            }
            0x1b => return Err("function pointer field".into()),
            _ => Ty::Never,
        })
    }

    fn full_name(a: &Assembly, t: u32) -> (String, String) {
        a.type_name((clr::TYPE_DEF, t)).unwrap_or_default()
    }

    /// The base type of a TypeDef with its generic arguments, or None at System.Object.
    fn base(&mut self, a: &Rc<Assembly>, t: u32, args: &[Ty]) -> Result<Option<Ty>, String> {
        let extends = a.coded(clr::TYPE_DEF, t, 3);
        if extends.1 == 0 {
            return Ok(None);
        }
        if extends.0 == clr::TYPE_SPEC {
            let blob = a.blob(clr::TYPE_SPEC, extends.1, 0).to_vec();
            return Ok(Some(self.sig_type(a, &mut blob.as_slice(), args)?));
        }
        let (r, b) = self.resolve(a, extends)?;
        Ok(Some(Ty::Def(r, b, Vec::new())))
    }

    fn derives_from(&mut self, a: &Rc<Assembly>, t: u32, namespace: &str, name: &str) -> Result<bool, String> {
        let (mut a, mut t) = (a.clone(), t);
        loop {
            if Self::full_name(&a, t) == (namespace.into(), name.into()) {
                return Ok(true);
            }
            match self.base(&a, t, &[])? {
                Some(Ty::Def(a2, t2, _)) => (a, t) = (a2, t2),
                _ => return Ok(false),
            }
        }
    }

    /// How a field of this type is serialized, or None when Unity skips it.
    fn kind(&mut self, ty: &Ty) -> Result<Option<Kind>, String> {
        Ok(match ty {
            Ty::Prim(e) => Some(Kind::Prim(*e)),
            Ty::String => Some(Kind::String),
            Ty::Array(e) => self.element(e)?.map(Kind::Array),
            Ty::Var(_) | Ty::Never => None,
            Ty::Def(a, t, args) => {
                let (namespace, name) = Self::full_name(a, *t);
                if (namespace.as_str(), name.as_str()) == ("System.Collections.Generic", "List`1") {
                    return Ok(self.element(&args[0])?.map(Kind::Array));
                }
                if let Some(n) = native(&namespace, &name) {
                    return Ok(Some(Kind::Native(n)));
                }
                if self.derives_from(a, *t, "System", "Enum")? {
                    let (start, end) = a.field_range(*t);
                    let value = (start..end).find(|&f| a.string(clr::FIELD, f, 1) == "value__").ok_or("enum without value__")?;
                    let sig = a.blob(clr::FIELD, value, 2).to_vec();
                    let mut b = &sig[1..];
                    let underlying = self.sig_type(a, &mut b, &[])?;
                    return self.kind(&underlying);
                }
                if self.derives_from(a, *t, "UnityEngine", "Object")? {
                    return Ok(Some(Kind::PPtr));
                }
                let flags = a.cell(clr::TYPE_DEF, *t, 0);
                const ABSTRACT: u32 = 0x80;
                const INTERFACE: u32 = 0x20;
                const SERIALIZABLE: u32 = 0x2000;
                if flags & SERIALIZABLE == 0
                    || flags & (ABSTRACT | INTERFACE) != 0
                    || namespace == "System"
                    || namespace.starts_with("System.")
                    || self.derives_from(a, *t, "System", "MulticastDelegate")?
                {
                    return Ok(None);
                }
                Some(Kind::Class(a.clone(), *t, args.clone()))
            }
        })
    }

    /// An array/List element type, if Unity serializes collections of it (no nested collections).
    fn element(&mut self, e: &Ty) -> Result<Option<Ty>, String> {
        Ok(match self.kind(e)? {
            None | Some(Kind::Array(_)) => None,
            Some(_) => Some(e.clone()),
        })
    }

    /// The serialized fields of a script class at `r` (just past the MonoBehaviour header).
    pub fn read_script(&mut self, assembly: &str, namespace: &str, class: &str, r: &mut Reader) -> Result<Value, String> {
        let (a, t) = self.find(assembly, namespace, class)?;
        let mut fields = Vec::new();
        self.read_fields(&a, t, &[], r, &mut fields, 0)?;
        Ok(Value::Struct(fields))
    }

    fn read_fields(
        &mut self,
        a: &Rc<Assembly>,
        t: u32,
        args: &[Ty],
        r: &mut Reader,
        out: &mut Vec<(String, Value)>,
        depth: u32,
    ) -> Result<(), String> {
        if let Some(Ty::Def(ba, bt, bargs)) = self.base(a, t, args)? {
            let stop = matches!(
                Self::full_name(&ba, bt),
                (ref n, ref c) if (n == "UnityEngine" && ["MonoBehaviour", "ScriptableObject", "Behaviour", "Component", "Object"].contains(&c.as_str()))
                    || (n == "System" && ["Object", "ValueType"].contains(&c.as_str()))
            );
            if !stop {
                self.read_fields(&ba, bt, &bargs, r, out, depth)?;
            }
        }
        let (start, end) = a.field_range(t);
        for f in start..end {
            let flags = a.cell(clr::FIELD, f, 0);
            const STATIC: u32 = 0x10;
            const INIT_ONLY: u32 = 0x20;
            const LITERAL: u32 = 0x40;
            const NOT_SERIALIZED: u32 = 0x80;
            const PUBLIC: u32 = 6;
            if flags & (STATIC | INIT_ONLY | LITERAL | NOT_SERIALIZED) != 0 {
                continue;
            }
            let token = (clr::FIELD, f);
            if a.has_attribute(token, "UnityEngine", "SerializeReference") {
                return Err(format!("[SerializeReference] {}", a.string(clr::FIELD, f, 1)));
            }
            if flags & 7 != PUBLIC && !a.has_attribute(token, "UnityEngine", "SerializeField") {
                continue;
            }
            let sig = a.blob(clr::FIELD, f, 2).to_vec();
            let mut b = &sig[1..];
            let ty = self.sig_type(a, &mut b, args)?;
            let Some(kind) = self.kind(&ty)? else { continue };
            let name = a.string(clr::FIELD, f, 1).to_string();
            let v = self.read_value(kind, r, depth).map_err(|e| format!("{name}: {e}"))?;
            out.push((name, v));
        }
        Ok(())
    }

    fn read_value(&mut self, kind: Kind, r: &mut Reader, depth: u32) -> Result<Value, String> {
        Ok(match kind {
            Kind::Prim(e) => {
                let v = prim(e, r)?;
                if matches!(e, 0x02..=0x07) {
                    r.align();
                }
                v
            }
            Kind::String => Value::Str(r.string()?),
            Kind::PPtr => Value::PPtr(PPtr::read(r)?),
            Kind::Native(n) => read_native(n, r)?,
            Kind::Array(e) => {
                let n = r.i32()?;
                if n < 0 || n as usize > r.data.len() {
                    return Err(format!("array length {n}"));
                }
                let mut v = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    // Primitive elements are packed; the array as a whole is aligned.
                    v.push(match self.kind(&e)?.unwrap() {
                        Kind::Prim(p) => prim(p, r)?,
                        k => self.read_value(k, r, depth)?,
                    });
                }
                r.align();
                Value::Array(v)
            }
            Kind::Class(a, t, args) => {
                if depth >= 10 {
                    return Err(format!("serialization depth over 10 at {:?}", Self::full_name(&a, t)));
                }
                let mut fields = Vec::new();
                self.read_fields(&a, t, &args, r, &mut fields, depth + 1)?;
                Value::Struct(fields)
            }
        })
    }
}

fn take(b: &mut &[u8]) -> Result<u8, String> {
    let (&first, rest) = b.split_first().ok_or("short signature")?;
    *b = rest;
    Ok(first)
}

fn compressed(b: &mut &[u8]) -> Result<u32, String> {
    let (v, n) = clr::compressed(b)?;
    *b = &b[n..];
    Ok(v)
}

fn prim(e: u8, r: &mut Reader) -> Result<Value, String> {
    Ok(match e {
        0x02 => Value::Bool(r.bool()?),
        0x03 | 0x07 => Value::UInt(r.u16()? as u64),
        0x04 => Value::Int(r.u8()? as i8 as i64),
        0x05 => Value::UInt(r.u8()? as u64),
        0x06 => Value::Int(r.i16()? as i64),
        0x08 => Value::Int(r.i32()? as i64),
        0x09 => Value::UInt(r.u32()? as u64),
        0x0a => Value::Int(r.i64()?),
        0x0b => Value::UInt(r.u64()?),
        0x0c => Value::Float(r.f32()?),
        0x0d => Value::Double(f64::from_bits(r.u64()?)),
        _ => return Err(format!("element type {e:#x}")),
    })
}

/// Engine types the serializer writes natively instead of by their managed fields.
fn native(namespace: &str, name: &str) -> Option<&'static str> {
    if namespace != "UnityEngine" {
        return None;
    }
    const NATIVE: &[&str] = &[
        "Vector2", "Vector3", "Vector4", "Quaternion", "Rect", "Color", "Color32", "LayerMask", "Bounds", "Matrix4x4",
        "Vector2Int", "Vector3Int", "RectInt", "BoundsInt", "AnimationCurve", "Gradient", "RectOffset", "GUIStyle",
        "Hash128", "PropertyName",
    ];
    NATIVE.iter().find(|&&n| n == name).copied()
}

fn floats(r: &mut Reader, names: &[&str]) -> Result<Value, String> {
    let mut v = Vec::new();
    for n in names {
        v.push((n.to_string(), Value::Float(r.f32()?)));
    }
    Ok(Value::Struct(v))
}

fn ints(r: &mut Reader, names: &[&str]) -> Result<Value, String> {
    let mut v = Vec::new();
    for n in names {
        v.push((n.to_string(), Value::Int(r.i32()? as i64)));
    }
    Ok(Value::Struct(v))
}

fn read_native(name: &str, r: &mut Reader) -> Result<Value, String> {
    Ok(match name {
        "Vector2" => floats(r, &["x", "y"])?,
        "Vector3" => floats(r, &["x", "y", "z"])?,
        "Vector4" | "Quaternion" => floats(r, &["x", "y", "z", "w"])?,
        "Rect" => floats(r, &["x", "y", "width", "height"])?,
        "Color" => floats(r, &["r", "g", "b", "a"])?,
        "Color32" => Value::Struct(vec![("rgba".into(), Value::UInt(r.u32()? as u64))]),
        "LayerMask" => Value::Struct(vec![("m_Bits".into(), Value::UInt(r.u32()? as u64))]),
        "Bounds" => Value::Struct(vec![
            ("m_Center".into(), floats(r, &["x", "y", "z"])?),
            ("m_Extent".into(), floats(r, &["x", "y", "z"])?),
        ]),
        "Matrix4x4" => {
            let names: Vec<String> = (0..16).map(|i| format!("e{}{}", i % 4, i / 4)).collect();
            floats(r, &names.iter().map(|s| s.as_str()).collect::<Vec<_>>())?
        }
        "Vector2Int" => ints(r, &["x", "y"])?,
        "Vector3Int" => ints(r, &["x", "y", "z"])?,
        "RectInt" => ints(r, &["x", "y", "width", "height"])?,
        "BoundsInt" => Value::Struct(vec![("m_Position".into(), ints(r, &["x", "y", "z"])?), ("m_Size".into(), ints(r, &["x", "y", "z"])?)]),
        "RectOffset" => ints(r, &["m_Left", "m_Right", "m_Top", "m_Bottom"])?,
        "AnimationCurve" => {
            let n = r.i32()?;
            let mut keys = Vec::new();
            for _ in 0..n {
                let mut k = Vec::new();
                for f in ["time", "value", "inSlope", "outSlope"] {
                    k.push((f.to_string(), Value::Float(r.f32()?)));
                }
                k.push(("weightedMode".into(), Value::Int(r.i32()? as i64)));
                k.push(("inWeight".into(), Value::Float(r.f32()?)));
                k.push(("outWeight".into(), Value::Float(r.f32()?)));
                keys.push(Value::Struct(k));
            }
            Value::Struct(vec![
                ("m_Curve".into(), Value::Array(keys)),
                ("m_PreInfinity".into(), Value::Int(r.i32()? as i64)),
                ("m_PostInfinity".into(), Value::Int(r.i32()? as i64)),
                ("m_RotationOrder".into(), Value::Int(r.i32()? as i64)),
            ])
        }
        "Gradient" => {
            let mut v = Vec::new();
            for i in 0..8 {
                v.push((format!("key{i}"), floats(r, &["r", "g", "b", "a"])?));
            }
            for p in ["ctime", "atime"] {
                for i in 0..8 {
                    v.push((format!("{p}{i}"), Value::UInt(r.u16()? as u64)));
                }
            }
            v.push(("m_Mode".into(), Value::Int(r.i32()? as i64)));
            v.push(("m_NumColorKeys".into(), Value::UInt(r.u8()? as u64)));
            v.push(("m_NumAlphaKeys".into(), Value::UInt(r.u8()? as u64)));
            r.align();
            Value::Struct(v)
        }
        _ => return Err(format!("native {name} layout not rewritten")),
    })
}

#[cfg(test)]
mod tests {
    use super::super::classes::{MonoBehaviour, MonoScript};
    use super::super::serialized::SerializedFile;
    use super::*;
    use std::collections::BTreeMap;

    /// Every MonoBehaviour in the shipped data reads to exactly its serialized size.
    #[test]
    fn all_monobehaviours_consume_exactly() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let data = game.join("People Playground_Data");
        let mut managed = Managed::new(&game);
        let mut names: Vec<String> = std::fs::read_dir(&data)
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| (n.ends_with(".assets") || n.starts_with("level") || n == "globalgamemanagers") && !n.ends_with(".resS"))
            .collect();
        names.sort();
        let files: HashMap<String, SerializedFile> =
            names.iter().map(|n| (n.clone(), SerializedFile::open(&data.join(n)).unwrap())).collect();
        let mut scripts: HashMap<(String, i64), MonoScript> = HashMap::new();
        let mut failures: BTreeMap<String, (usize, String)> = BTreeMap::new();
        let (mut ok, mut missing) = (0, 0);
        let mut limbs = Vec::new();
        for name in &names {
            let f = &files[name];
            for o in f.objects.iter().filter(|o| o.class_id == 114) {
                let d = f.data(o);
                let mb = MonoBehaviour::read(d).unwrap();
                if mb.script.is_null() {
                    continue;
                }
                let script_file = if mb.script.file == 0 { name.clone() } else { f.externals[mb.script.file as usize - 1].path.clone() };
                let script_file = script_file.rsplit('/').next().unwrap().to_string();
                let script = scripts.entry((script_file.clone(), mb.script.path_id)).or_insert_with(|| {
                    let sf = &files[&script_file];
                    MonoScript::read(sf.data(sf.object(mb.script.path_id).unwrap())).unwrap()
                });
                let mut r = Reader::new(d);
                r.pos = mb.fields;
                let class = format!("{}:{}.{}", script.assembly, script.namespace, script.class_name);
                // Unity keeps the data of a component whose class was removed from the build and loads it as
                // "missing script"; there is nothing to read it with.
                if managed.assembly(&script.assembly).unwrap().type_def(&script.namespace, &script.class_name).is_none() {
                    missing += 1;
                    continue;
                }
                if script.class_name == "LimbBehaviour" {
                    let mut r = Reader::new(d);
                    r.pos = mb.fields;
                    limbs.push(managed.read_script(&script.assembly, "", "LimbBehaviour", &mut r).unwrap());
                }
                let err = match managed.read_script(&script.assembly, &script.namespace, &script.class_name, &mut r) {
                    Ok(_) if r.pos == d.len() => {
                        ok += 1;
                        continue;
                    }
                    Ok(_) => format!("read {} of {} bytes", r.pos, d.len()),
                    Err(e) => e,
                };
                let e = failures.entry(class).or_insert((0, err));
                e.0 += 1;
            }
        }
        for (class, (n, err)) in &failures {
            eprintln!("{n:5} {class}: {err}");
        }
        assert!(failures.is_empty(), "{ok} ok, {} classes fail", failures.len());
        assert!(ok > 9000 && missing < 10, "{ok} ok, {missing} missing scripts");
        // Person.prefab's head, as the AssetRipper export of the same data shows it.
        let head = limbs
            .iter()
            .find(|l| {
                l.get("HasBrain") == Some(&Value::Bool(true))
                    && l.get("SpeciesIdentity") == Some(&Value::Str("Human".into()))
                    && l.get("BaseStrength").and_then(Value::f32) == Some(8.0)
                    && l.get("BreakingThreshold").and_then(Value::f32) == Some(5.0)
            })
            .expect("human head LimbBehaviour");
        let f = |n: &str| head.get(n).and_then(Value::f32).unwrap();
        assert_eq!(
            [f("Vitality"), f("RegenerationSpeed"), f("ImpactPainMultiplier"), f("ExplodeLiquidAmount"), f("Health")],
            [7.0, 0.1, 5.0, 2.6785715, 50.0]
        );
        assert_eq!(head.get("BloodLiquidType"), Some(&Value::Str("BLOOD".into())));
        let vital = head.get("VitalParts").and_then(Value::array).unwrap();
        assert_eq!(vital[0].get("m_Center").and_then(|c| c.get("y")).and_then(Value::f32), Some(0.09));
        assert_eq!(head.get("ConnectedLimbs").and_then(Value::array).map(|a| a.len()), Some(1));
    }
}
