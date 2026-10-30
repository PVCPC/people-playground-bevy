//! ECMA-335 metadata of the game's managed assemblies (Managed/*.dll): type definitions, fields and the custom
//! attributes Unity's serializer looks at. Only reading; no IL.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const TYPE_REF: usize = 0x01;
pub const TYPE_DEF: usize = 0x02;
pub const FIELD: usize = 0x04;
pub const METHOD_DEF: usize = 0x06;
pub const MEMBER_REF: usize = 0x0A;
pub const CUSTOM_ATTRIBUTE: usize = 0x0C;
pub const TYPE_SPEC: usize = 0x1B;
pub const ASSEMBLY_REF: usize = 0x23;
pub const EXPORTED_TYPE: usize = 0x27;
pub const NESTED_CLASS: usize = 0x29;

/// Column kinds of the metadata tables.
#[derive(Clone, Copy)]
enum Col {
    U16,
    U32,
    Str,
    Guid,
    Blob,
    /// A simple index into table n.
    T(usize),
    /// A coded index over these tables (usize::MAX = unused tag).
    C(&'static [usize]),
}

const NA: usize = usize::MAX;
const TYPE_DEF_OR_REF: &[usize] = &[0x02, 0x01, 0x1B];
const HAS_CONSTANT: &[usize] = &[0x04, 0x08, 0x17];
const HAS_CUSTOM_ATTRIBUTE: &[usize] =
    &[0x06, 0x04, 0x01, 0x02, 0x08, 0x09, 0x0A, 0x00, 0x0E, 0x17, 0x14, 0x11, 0x1A, 0x1B, 0x20, 0x23, 0x26, 0x27, 0x28, 0x2A, 0x2C, 0x2B];
const HAS_FIELD_MARSHAL: &[usize] = &[0x04, 0x08];
const HAS_DECL_SECURITY: &[usize] = &[0x02, 0x06, 0x20];
const MEMBER_REF_PARENT: &[usize] = &[0x02, 0x01, 0x1A, 0x06, 0x1B];
const HAS_SEMANTICS: &[usize] = &[0x14, 0x17];
const METHOD_DEF_OR_REF: &[usize] = &[0x06, 0x0A];
const MEMBER_FORWARDED: &[usize] = &[0x04, 0x06];
const IMPLEMENTATION: &[usize] = &[0x26, 0x23, 0x27];
const CUSTOM_ATTRIBUTE_TYPE: &[usize] = &[NA, NA, 0x06, 0x0A, NA];
const RESOLUTION_SCOPE: &[usize] = &[0x00, 0x1A, 0x23, 0x01];
const TYPE_OR_METHOD_DEF: &[usize] = &[0x02, 0x06];

fn schema(table: usize) -> &'static [Col] {
    use Col::*;
    match table {
        0x00 => &[U16, Str, Guid, Guid, Guid],
        0x01 => &[C(RESOLUTION_SCOPE), Str, Str],
        0x02 => &[U32, Str, Str, C(TYPE_DEF_OR_REF), T(0x04), T(0x06)],
        0x03 => &[T(0x04)],
        0x04 => &[U16, Str, Blob],
        0x05 => &[T(0x06)],
        0x06 => &[U32, U16, U16, Str, Blob, T(0x08)],
        0x07 => &[T(0x08)],
        0x08 => &[U16, U16, Str],
        0x09 => &[T(0x02), C(TYPE_DEF_OR_REF)],
        0x0A => &[C(MEMBER_REF_PARENT), Str, Blob],
        0x0B => &[U16, C(HAS_CONSTANT), Blob],
        0x0C => &[C(HAS_CUSTOM_ATTRIBUTE), C(CUSTOM_ATTRIBUTE_TYPE), Blob],
        0x0D => &[C(HAS_FIELD_MARSHAL), Blob],
        0x0E => &[U16, C(HAS_DECL_SECURITY), Blob],
        0x0F => &[U16, U32, T(0x02)],
        0x10 => &[U32, T(0x04)],
        0x11 => &[Blob],
        0x12 => &[T(0x02), T(0x14)],
        0x13 => &[T(0x14)],
        0x14 => &[U16, Str, C(TYPE_DEF_OR_REF)],
        0x15 => &[T(0x02), T(0x17)],
        0x16 => &[T(0x17)],
        0x17 => &[U16, Str, Blob],
        0x18 => &[U16, T(0x06), C(HAS_SEMANTICS)],
        0x19 => &[T(0x02), C(METHOD_DEF_OR_REF), C(METHOD_DEF_OR_REF)],
        0x1A => &[Str],
        0x1B => &[Blob],
        0x1C => &[U16, C(MEMBER_FORWARDED), Str, T(0x1A)],
        0x1D => &[U32, T(0x04)],
        0x1E => &[U32, U32],
        0x1F => &[U32],
        0x20 => &[U32, U16, U16, U16, U16, U32, Blob, Str, Str],
        0x21 => &[U32],
        0x22 => &[U32, U32, U32],
        0x23 => &[U16, U16, U16, U16, U32, Blob, Str, Str, Blob],
        0x24 => &[U32, T(0x23)],
        0x25 => &[U32, U32, U32, T(0x23)],
        0x26 => &[U32, Str, Blob],
        0x27 => &[U32, U32, Str, Str, C(IMPLEMENTATION)],
        0x28 => &[U32, U32, Str, C(IMPLEMENTATION)],
        0x29 => &[T(0x02), T(0x02)],
        0x2A => &[U16, U16, C(TYPE_OR_METHOD_DEF), Str],
        0x2B => &[C(METHOD_DEF_OR_REF), Blob],
        0x2C => &[T(0x2A), C(TYPE_DEF_OR_REF)],
        _ => &[],
    }
}

/// A decoded coded index: (table, 1-based row).
pub type Token = (usize, u32);

pub struct Assembly {
    pub path: PathBuf,
    bytes: Vec<u8>,
    strings: usize,
    blobs: usize,
    wide_str: bool,
    wide_guid: bool,
    wide_blob: bool,
    rows: [u32; 64],
    table_start: [usize; 64],
    row_size: [usize; 64],
    /// (namespace, name) -> TypeDef row, top-level types only; nested types keyed as "Outer/Inner".
    types: HashMap<(String, String), u32>,
    /// Field row -> owning TypeDef row.
    field_owner: Vec<u32>,
    /// Custom attributes by parent token.
    attributes: HashMap<Token, Vec<Token>>,
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// A compressed unsigned integer (II.23.2) and its length.
pub fn compressed(b: &[u8]) -> Result<(u32, usize), String> {
    let first = *b.first().ok_or("empty signature")?;
    Ok(if first & 0x80 == 0 {
        (first as u32, 1)
    } else if first & 0xC0 == 0x80 {
        ((((first & 0x3F) as u32) << 8) | *b.get(1).ok_or("short signature")? as u32, 2)
    } else {
        let b = b.get(..4).ok_or("short signature")?;
        ((((first & 0x1F) as u32) << 24) | (b[1] as u32) << 16 | (b[2] as u32) << 8 | b[3] as u32, 4)
    })
}

impl Assembly {
    pub fn open(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let b = &bytes;
        let pe = u32_at(b, 0x3C) as usize;
        if b.get(pe..pe + 4) != Some(b"PE\0\0") {
            return Err(format!("{}: not a PE file", path.display()));
        }
        let sections = u16_at(b, pe + 6) as usize;
        let optional = pe + 24;
        let optional_size = u16_at(b, pe + 20) as usize;
        let dirs = optional + if u16_at(b, optional) == 0x20b { 112 } else { 96 };
        let cli_rva = u32_at(b, dirs + 14 * 8);
        let section_table = optional + optional_size;
        let to_offset = |rva: u32| -> Result<usize, String> {
            for i in 0..sections {
                let s = section_table + i * 40;
                let (va, size, raw) = (u32_at(b, s + 12), u32_at(b, s + 8).max(u32_at(b, s + 16)), u32_at(b, s + 20));
                if rva >= va && rva < va + size {
                    return Ok((rva - va + raw) as usize);
                }
            }
            Err(format!("RVA {rva:#x} outside the sections"))
        };
        let cli = to_offset(cli_rva)?;
        let root = to_offset(u32_at(b, cli + 8))?;
        if u32_at(b, root) != 0x424A5342 {
            return Err(format!("{}: no metadata root", path.display()));
        }
        let version_len = u32_at(b, root + 12) as usize;
        let mut o = root + 16 + version_len + 2;
        let streams = u16_at(b, o) as usize;
        o += 2;
        let (mut tables, mut strings, mut blobs) = (0, 0, 0);
        for _ in 0..streams {
            let offset = root + u32_at(b, o) as usize;
            let name_start = o + 8;
            let name_len = b[name_start..].iter().position(|&c| c == 0).unwrap();
            match &b[name_start..name_start + name_len] {
                b"#~" | b"#-" => tables = offset,
                b"#Strings" => strings = offset,
                b"#Blob" => blobs = offset,
                _ => {}
            }
            o = name_start + ((name_len + 4) & !3);
        }
        let heaps = b[tables + 6];
        let valid = u64::from_le_bytes(b[tables + 8..tables + 16].try_into().unwrap());
        let mut rows = [0u32; 64];
        let mut o = tables + 24;
        for (t, r) in rows.iter_mut().enumerate() {
            if valid >> t & 1 != 0 {
                *r = u32_at(b, o);
                o += 4;
            }
        }
        let (wide_str, wide_guid, wide_blob) = (heaps & 1 != 0, heaps & 2 != 0, heaps & 4 != 0);
        let col_size = |c: Col| match c {
            Col::U16 => 2,
            Col::U32 => 4,
            Col::Str => 2 + 2 * wide_str as usize,
            Col::Guid => 2 + 2 * wide_guid as usize,
            Col::Blob => 2 + 2 * wide_blob as usize,
            Col::T(t) => 2 + 2 * (rows[t] > 0xFFFF) as usize,
            Col::C(ts) => {
                let bits = usize::BITS - (ts.len() - 1).leading_zeros();
                let max = ts.iter().filter(|&&t| t != NA).map(|&t| rows[t]).max().unwrap_or(0);
                2 + 2 * (max >= 1 << (16 - bits)) as usize
            }
        };
        let (mut table_start, mut row_size) = ([0usize; 64], [0usize; 64]);
        for t in 0..64 {
            if valid >> t & 1 == 0 {
                continue;
            }
            if t > 0x2C {
                return Err(format!("{}: unknown metadata table {t:#x}", path.display()));
            }
            row_size[t] = schema(t).iter().map(|&c| col_size(c)).sum();
            table_start[t] = o;
            o += row_size[t] * rows[t] as usize;
        }
        let mut a = Self {
            path: path.into(),
            bytes,
            strings,
            blobs,
            wide_str,
            wide_guid,
            wide_blob,
            rows,
            table_start,
            row_size,
            types: HashMap::new(),
            field_owner: Vec::new(),
            attributes: HashMap::new(),
        };
        a.index();
        Ok(a)
    }

    pub fn rows(&self, table: usize) -> u32 {
        self.rows[table]
    }

    /// Column `col` of `row` (1-based): the raw value, coded indices decoded with `token`.
    pub fn cell(&self, table: usize, row: u32, col: usize) -> u32 {
        let cols = schema(table);
        let mut o = self.table_start[table] + (row as usize - 1) * self.row_size[table];
        for (i, &c) in cols.iter().enumerate() {
            let size = match c {
                Col::U16 => 2,
                Col::U32 => 4,
                Col::Str => 2 + 2 * self.wide_str as usize,
                Col::Guid => 2 + 2 * self.wide_guid as usize,
                Col::Blob => 2 + 2 * self.wide_blob as usize,
                Col::T(t) => 2 + 2 * (self.rows[t] > 0xFFFF) as usize,
                Col::C(ts) => {
                    let bits = usize::BITS - (ts.len() - 1).leading_zeros();
                    let max = ts.iter().filter(|&&t| t != NA).map(|&t| self.rows[t]).max().unwrap_or(0);
                    2 + 2 * (max >= 1 << (16 - bits)) as usize
                }
            };
            if i == col {
                return if size == 2 { u16_at(&self.bytes, o) as u32 } else { u32_at(&self.bytes, o) };
            }
            o += size;
        }
        panic!("column {col} past table {table:#x}");
    }

    /// A coded-index column decoded to (table, row); row 0 = null.
    pub fn coded(&self, table: usize, row: u32, col: usize) -> Token {
        let Col::C(ts) = schema(table)[col] else { panic!("column {col} of {table:#x} is not coded") };
        let bits = usize::BITS - (ts.len() - 1).leading_zeros();
        let v = self.cell(table, row, col);
        (ts[(v & ((1 << bits) - 1)) as usize], v >> bits)
    }

    pub fn string(&self, table: usize, row: u32, col: usize) -> &str {
        let o = self.strings + self.cell(table, row, col) as usize;
        let n = self.bytes[o..].iter().position(|&c| c == 0).unwrap();
        std::str::from_utf8(&self.bytes[o..o + n]).unwrap_or("")
    }

    pub fn blob(&self, table: usize, row: u32, col: usize) -> &[u8] {
        let o = self.blobs + self.cell(table, row, col) as usize;
        let (n, len) = compressed(&self.bytes[o..]).unwrap();
        &self.bytes[o + len..o + len + n as usize]
    }

    /// A TypeDefOrRefEncoded value from a signature.
    pub fn type_def_or_ref(v: u32) -> Token {
        (TYPE_DEF_OR_REF[(v & 3) as usize], v >> 2)
    }

    fn index(&mut self) {
        let mut nested_in = HashMap::new();
        for r in 1..=self.rows[NESTED_CLASS] {
            nested_in.insert(self.cell(NESTED_CLASS, r, 0), self.cell(NESTED_CLASS, r, 1));
        }
        for t in 1..=self.rows[TYPE_DEF] {
            let mut name = self.string(TYPE_DEF, t, 1).to_string();
            let mut outer = t;
            while let Some(&o) = nested_in.get(&outer) {
                name = format!("{}/{name}", self.string(TYPE_DEF, o, 1));
                outer = o;
            }
            let namespace = self.string(TYPE_DEF, outer, 2).to_string();
            self.types.insert((namespace, name), t);
        }
        self.field_owner = vec![0; self.rows[FIELD] as usize + 1];
        for t in 1..=self.rows[TYPE_DEF] {
            let (start, end) = self.field_range(t);
            for f in start..end {
                self.field_owner[f as usize] = t;
            }
        }
        for r in 1..=self.rows[CUSTOM_ATTRIBUTE] {
            let parent = self.coded(CUSTOM_ATTRIBUTE, r, 0);
            let ctor = self.coded(CUSTOM_ATTRIBUTE, r, 1);
            self.attributes.entry(parent).or_default().push(ctor);
        }
    }

    pub fn type_def(&self, namespace: &str, name: &str) -> Option<u32> {
        self.types.get(&(namespace.to_string(), name.to_string())).copied()
    }

    /// The TypeDef's fields, [start, end) rows of the Field table.
    pub fn field_range(&self, t: u32) -> (u32, u32) {
        let start = self.cell(TYPE_DEF, t, 4);
        let end = if t < self.rows[TYPE_DEF] { self.cell(TYPE_DEF, t + 1, 4) } else { self.rows[FIELD] + 1 };
        (start, end)
    }

    /// (namespace, name) of the class that declares a custom attribute's constructor.
    pub fn attribute_class(&self, ctor: Token) -> Option<(String, String)> {
        let class = match ctor.0 {
            METHOD_DEF => (TYPE_DEF, self.method_owner(ctor.1)?),
            MEMBER_REF => self.coded(MEMBER_REF, ctor.1, 0),
            _ => return None,
        };
        self.type_name(class)
    }

    fn method_owner(&self, m: u32) -> Option<u32> {
        (1..=self.rows[TYPE_DEF]).rev().find(|&t| self.cell(TYPE_DEF, t, 5) <= m)
    }

    /// (namespace, name) of a TypeDef or TypeRef.
    pub fn type_name(&self, t: Token) -> Option<(String, String)> {
        match t.0 {
            TYPE_DEF => self.types.iter().find(|(_, r)| **r == t.1).map(|(k, _)| k.clone()),
            TYPE_REF => {
                let mut name = self.string(TYPE_REF, t.1, 1).to_string();
                let mut cur = t.1;
                while let (TYPE_REF, outer) = self.coded(TYPE_REF, cur, 0) {
                    name = format!("{}/{name}", self.string(TYPE_REF, outer, 1));
                    cur = outer;
                }
                Some((self.string(TYPE_REF, cur, 2).to_string(), name))
            }
            _ => None,
        }
    }

    /// The assembly a TypeRef resolves into (None = this one).
    pub fn type_ref_assembly(&self, r: u32) -> Option<String> {
        let mut cur = r;
        loop {
            match self.coded(TYPE_REF, cur, 0) {
                (TYPE_REF, outer) => cur = outer,
                (ASSEMBLY_REF, a) => return Some(self.string(ASSEMBLY_REF, a, 6).to_string()),
                _ => return None,
            }
        }
    }

    /// The assembly an ExportedType forwards (namespace, name) to.
    pub fn forwarded(&self, namespace: &str, name: &str) -> Option<String> {
        (1..=self.rows[EXPORTED_TYPE])
            .find(|&r| self.string(EXPORTED_TYPE, r, 2) == name && self.string(EXPORTED_TYPE, r, 3) == namespace)
            .and_then(|r| match self.coded(EXPORTED_TYPE, r, 4) {
                (ASSEMBLY_REF, a) => Some(self.string(ASSEMBLY_REF, a, 6).to_string()),
                _ => None,
            })
    }

    pub fn attributes(&self, parent: Token) -> &[Token] {
        self.attributes.get(&parent).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn has_attribute(&self, parent: Token, namespace: &str, name: &str) -> bool {
        self.attributes(parent)
            .iter()
            .any(|&c| self.attribute_class(c).is_some_and(|(n, c)| n == namespace && c == name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembly_csharp_types() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let a = Assembly::open(&game.join("People Playground_Data/Managed/Assembly-CSharp.dll")).unwrap();
        let t = a.type_def("", "LimbBehaviour").expect("LimbBehaviour");
        let (start, end) = a.field_range(t);
        let names: Vec<&str> = (start..end).map(|f| a.string(FIELD, f, 1)).collect();
        assert!(names.contains(&"Health") && names.contains(&"InitialHealth"), "{names:?}");
        assert_eq!(a.type_name((TYPE_DEF, t)).unwrap(), (String::new(), "LimbBehaviour".into()));
    }
}
