//! Unity SerializedFile (format 22, Unity 2020.3): the object table of the installed game's .assets files. Player
//! builds strip the type trees, so object data is read with the class layouts in `classes`.

use std::path::{Path, PathBuf};

/// One serialized type: its class id and, for a MonoBehaviour, the script's hash.
#[derive(Clone, Debug)]
pub struct SerializedType {
    pub class_id: i32,
    pub stripped: bool,
    pub script_type_index: i16,
    pub script_id: Option<[u8; 16]>,
    pub old_type_hash: [u8; 16],
}

#[derive(Clone, Debug)]
pub struct ObjectInfo {
    pub path_id: i64,
    pub byte_start: u64,
    pub byte_size: u32,
    pub type_index: i32,
    pub class_id: i32,
}

/// A PPtr's target file: 0 is this file, n is externals[n - 1].
#[derive(Clone, Debug)]
pub struct External {
    pub guid: [u8; 16],
    pub kind: i32,
    pub path: String,
}

#[derive(Debug)]
pub struct SerializedFile {
    pub path: PathBuf,
    pub version: u32,
    pub unity_version: String,
    pub platform: u32,
    pub types: Vec<SerializedType>,
    pub objects: Vec<ObjectInfo>,
    /// (file index, path id) of each script type.
    pub script_types: Vec<(i32, i64)>,
    pub externals: Vec<External>,
    data_offset: u64,
    bytes: Vec<u8>,
}

/// A little-endian (or big-endian header) cursor with Unity's 4-byte alignment.
pub struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
    pub big: bool,
}

macro_rules! read_num {
    ($name:ident, $t:ty) => {
        pub fn $name(&mut self) -> Result<$t, String> {
            const N: usize = std::mem::size_of::<$t>();
            let b: [u8; N] = self.data.get(self.pos..self.pos + N).ok_or("read past the end")?.try_into().unwrap();
            self.pos += N;
            Ok(if self.big { <$t>::from_be_bytes(b) } else { <$t>::from_le_bytes(b) })
        }
    };
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, big: false }
    }
    read_num!(u8, u8);
    read_num!(i16, i16);
    read_num!(u16, u16);
    read_num!(i32, i32);
    read_num!(u32, u32);
    read_num!(i64, i64);
    read_num!(u64, u64);
    read_num!(f32, f32);

    pub fn bool(&mut self) -> Result<bool, String> {
        Ok(self.u8()? != 0)
    }
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let b = self.data.get(self.pos..self.pos + n).ok_or("read past the end")?;
        self.pos += n;
        Ok(b)
    }
    pub fn align(&mut self) {
        self.pos = (self.pos + 3) & !3;
    }
    pub fn cstring(&mut self) -> Result<String, String> {
        let rest = &self.data[self.pos..];
        let n = rest.iter().position(|&b| b == 0).ok_or("unterminated string")?;
        self.pos += n + 1;
        String::from_utf8(rest[..n].to_vec()).map_err(|e| e.to_string())
    }
    /// A serialized string: i32 length, bytes, align.
    pub fn string(&mut self) -> Result<String, String> {
        let n = self.i32()?;
        let n = usize::try_from(n).map_err(|_| "negative string length")?;
        let s = String::from_utf8_lossy(self.bytes(n)?).into_owned();
        self.align();
        Ok(s)
    }
    pub fn guid(&mut self) -> Result<[u8; 16], String> {
        Ok(self.bytes(16)?.try_into().unwrap())
    }
}

impl SerializedFile {
    pub fn open(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut r = Reader { data: &bytes, pos: 0, big: true };
        let (_metadata_size, _file_size, version, _data_offset) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
        if version != 22 {
            return Err(format!("{}: SerializedFile format {version}, expected 22", path.display()));
        }
        let endian = r.u8()?;
        r.bytes(3)?;
        let (_metadata_size, _file_size, data_offset, _) = (r.u32()?, r.u64()?, r.u64()?, r.u64()?);
        r.big = endian != 0;
        let unity_version = r.cstring()?;
        let platform = r.u32()?;
        if r.bool()? {
            return Err(format!("{}: type trees present; only stripped player data is read", path.display()));
        }
        let mut types = Vec::new();
        for _ in 0..r.i32()? {
            let class_id = r.i32()?;
            let stripped = r.bool()?;
            let script_type_index = r.i16()?;
            let script_id = if class_id == 114 { Some(r.guid()?) } else { None };
            types.push(SerializedType { class_id, stripped, script_type_index, script_id, old_type_hash: r.guid()? });
        }
        let mut objects = Vec::new();
        for _ in 0..r.i32()? {
            r.align();
            let path_id = r.i64()?;
            let byte_start = r.u64()?;
            let byte_size = r.u32()?;
            let type_index = r.i32()?;
            let class_id = types.get(type_index as usize).ok_or("object type out of range")?.class_id;
            objects.push(ObjectInfo { path_id, byte_start, byte_size, type_index, class_id });
        }
        let mut script_types = Vec::new();
        for _ in 0..r.i32()? {
            let file = r.i32()?;
            r.align();
            script_types.push((file, r.i64()?));
        }
        let mut externals = Vec::new();
        for _ in 0..r.i32()? {
            let _empty = r.cstring()?;
            let guid = r.guid()?;
            let kind = r.i32()?;
            externals.push(External { guid, kind, path: r.cstring()? });
        }
        Ok(Self { path: path.into(), version, unity_version, platform, types, objects, script_types, externals, data_offset, bytes })
    }

    pub fn object(&self, path_id: i64) -> Option<&ObjectInfo> {
        self.objects.iter().find(|o| o.path_id == path_id)
    }

    /// An object's serialized bytes (little-endian, as every player platform this game ships on).
    pub fn data(&self, o: &ObjectInfo) -> &[u8] {
        let start = (self.data_offset + o.byte_start) as usize;
        &self.bytes[start..start + o.byte_size as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The installed game's resources.assets parses, and every object lies inside the file.
    #[test]
    fn installed_resources_assets() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let f = SerializedFile::open(&game.join("People Playground_Data/resources.assets")).unwrap();
        assert_eq!(f.unity_version, "2020.3.1f1");
        assert!(!f.objects.is_empty());
        for o in &f.objects {
            assert!(f.data_offset + o.byte_start + o.byte_size as u64 <= f.bytes.len() as u64);
        }
        // GameObjects, Transforms and MonoBehaviours make up the prefabs.
        for class in [1, 4, 114] {
            assert!(f.objects.iter().any(|o| o.class_id == class), "no class {class}");
        }
    }
}
