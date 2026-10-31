//! Unity 2020.3 class layouts for stripped player data (release build fields only).

use super::serialized::Reader;

/// PPtr<T>: m_FileID (0 = this file, n = externals[n - 1]) and m_PathID.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PPtr {
    pub file: i32,
    pub path_id: i64,
}

impl PPtr {
    pub fn read(r: &mut Reader) -> Result<Self, String> {
        Ok(Self { file: r.i32()?, path_id: r.i64()? })
    }
    pub fn is_null(&self) -> bool {
        self.path_id == 0
    }
}

fn vec<T>(r: &mut Reader, mut item: impl FnMut(&mut Reader) -> Result<T, String>) -> Result<Vec<T>, String> {
    let n = r.i32()?;
    (0..n).map(|_| item(r)).collect()
}

fn floats<const N: usize>(r: &mut Reader) -> Result<[f32; N], String> {
    let mut v = [0.0; N];
    for x in &mut v {
        *x = r.f32()?;
    }
    Ok(v)
}

/// GameObject (class 1).
#[derive(Clone, Debug)]
pub struct GameObject {
    pub components: Vec<PPtr>,
    pub layer: u32,
    pub name: String,
    pub tag: u16,
    pub active: bool,
}

impl GameObject {
    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        let components = vec(r, PPtr::read)?;
        let layer = r.u32()?;
        let name = r.string()?;
        let tag = r.u16()?;
        let active = r.bool()?;
        Ok(Self { components, layer, name, tag, active })
    }
}

/// Transform (class 4).
#[derive(Clone, Debug)]
pub struct Transform {
    pub game_object: PPtr,
    pub local_rotation: [f32; 4],
    pub local_position: [f32; 3],
    pub local_scale: [f32; 3],
    pub children: Vec<PPtr>,
    pub father: PPtr,
}

impl Transform {
    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        Ok(Self {
            game_object: PPtr::read(r)?,
            local_rotation: floats(r)?,
            local_position: floats(r)?,
            local_scale: floats(r)?,
            children: vec(r, PPtr::read)?,
            father: PPtr::read(r)?,
        })
    }
}

/// MonoBehaviour (class 114): the native header; the script's serialized fields follow at `fields`.
#[derive(Clone, Debug)]
pub struct MonoBehaviour {
    pub game_object: PPtr,
    pub enabled: bool,
    pub script: PPtr,
    pub name: String,
    pub fields: usize,
}

impl MonoBehaviour {
    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        let game_object = PPtr::read(r)?;
        let enabled = r.bool()?;
        r.align();
        let script = PPtr::read(r)?;
        let name = r.string()?;
        Ok(Self { game_object, enabled, script, name, fields: r.pos })
    }
}

/// MonoScript (class 115).
#[derive(Clone, Debug)]
pub struct MonoScript {
    pub name: String,
    pub execution_order: i32,
    pub class_name: String,
    pub namespace: String,
    pub assembly: String,
}

impl MonoScript {
    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        let name = r.string()?;
        let execution_order = r.i32()?;
        let _properties_hash = r.bytes(16)?;
        Ok(Self { name, execution_order, class_name: r.string()?, namespace: r.string()?, assembly: r.string()? })
    }
}

#[cfg(test)]
mod tests {
    use super::super::serialized::SerializedFile;
    use super::*;

    fn data_file(name: &str) -> Option<SerializedFile> {
        let game = crate::game_assets::find_game(None).ok()?;
        Some(SerializedFile::open(&game.join("People Playground_Data").join(name)).unwrap())
    }

    /// BleedingParticle.prefab's root Transform, as the AssetRipper export of the same file shows it.
    #[test]
    fn bleeding_particle_prefab() {
        let Some(f) = data_file("sharedassets0.assets") else { return };
        let go = f
            .objects
            .iter()
            .filter(|o| o.class_id == 1)
            .map(|o| GameObject::read(f.data(o)).unwrap())
            .find(|g| g.name == "BleedingParticle")
            .expect("BleedingParticle");
        let t = go.components.iter().map(|c| f.object(c.path_id).unwrap()).find(|o| o.class_id == 4).unwrap();
        let t = Transform::read(f.data(t)).unwrap();
        assert_eq!(t.local_position, [-2.729651, 0.16047063, 0.0]);
        assert_eq!(t.local_rotation, [0.0, 0.0, -0.7069588, 0.70725477]);
        assert_eq!(t.local_scale, [1.0; 3]);
        assert!(t.father.is_null());
        let names: Vec<String> = t
            .children
            .iter()
            .map(|c| Transform::read(f.data(f.object(c.path_id).unwrap())).unwrap().game_object)
            .map(|g| GameObject::read(f.data(f.object(g.path_id).unwrap())).unwrap().name)
            .collect();
        assert_eq!(names, ["Trail", "extraDecals", "water puff"]);
        // Its scripts resolve to MonoScripts in another file.
        let mb = go.components.iter().map(|c| f.object(c.path_id).unwrap()).find(|o| o.class_id == 114).unwrap();
        let mb = MonoBehaviour::read(f.data(mb)).unwrap();
        assert!(mb.script.file > 0);
        let ext = &f.externals[mb.script.file as usize - 1];
        let Some(g) = data_file(&ext.path) else { panic!("{}", ext.path) };
        let script = MonoScript::read(g.data(g.object(mb.script.path_id).unwrap())).unwrap();
        assert!(script.assembly.starts_with("Assembly-CSharp"), "{script:?}");
    }
}

/// AudioClip (83), up to m_Length.
pub struct AudioClip {
    pub frequency: i32,
    /// AudioClip.length, in seconds.
    pub length: f32,
}

impl AudioClip {
    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        let _name = r.string()?;
        let (_load_type, _channels, frequency, _bits_per_sample) = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
        let length = r.f32()?;
        if !(1000..=384000).contains(&frequency) || !(0.0..3600.0).contains(&length) {
            return Err(format!("AudioClip: frequency {frequency}, length {length}"));
        }
        Ok(Self { frequency, length })
    }
}
