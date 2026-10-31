//! Unity 2020.3 physics and rendering component layouts (stripped player data). Every reader rejects data it does
//! not consume exactly.

use super::classes::PPtr;
use super::serialized::Reader;

fn floats<const N: usize>(r: &mut Reader) -> Result<[f32; N], String> {
    let mut v = [0.0; N];
    for x in &mut v {
        *x = r.f32()?;
    }
    Ok(v)
}

fn vec2(r: &mut Reader) -> Result<[f32; 2], String> {
    floats(r)
}

fn vec<T>(r: &mut Reader, mut item: impl FnMut(&mut Reader) -> Result<T, String>) -> Result<Vec<T>, String> {
    let n = r.i32()?;
    (0..n).map(|_| item(r)).collect()
}

/// A bool followed by 4-byte alignment.
fn flag(r: &mut Reader) -> Result<bool, String> {
    let v = r.bool()?;
    r.align();
    Ok(v)
}

fn done<T>(r: &Reader, what: &str, v: T) -> Result<T, String> {
    if r.pos != r.data.len() {
        return Err(format!("{what}: read {} of {} bytes", r.pos, r.data.len()));
    }
    Ok(v)
}

/// PointEffector2D (class 250): Behaviour, Effector2D (useColliderMask, colliderMask), then the point fields.
#[derive(Clone, Debug)]
pub struct PointEffector2D {
    pub game_object: PPtr,
    pub enabled: bool,
    pub use_collider_mask: bool,
    pub collider_mask: i32,
    pub force_magnitude: f32,
    pub force_variation: f32,
    pub distance_scale: f32,
    pub force_source: i32,
    pub force_target: i32,
    pub force_mode: i32,
    pub drag: f32,
    pub angular_drag: f32,
}

impl PointEffector2D {
    pub const CLASS: i32 = 250;

    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        let game_object = PPtr::read(r)?;
        let enabled = flag(r)?;
        let use_collider_mask = flag(r)?;
        let collider_mask = r.i32()?;
        let force_magnitude = r.f32()?;
        let force_variation = r.f32()?;
        let distance_scale = r.f32()?;
        let (force_source, force_target, force_mode) = (r.i32()?, r.i32()?, r.i32()?);
        let (drag, angular_drag) = (r.f32()?, r.f32()?);
        done(r, "PointEffector2D", PointEffector2D { game_object, enabled, use_collider_mask, collider_mask, force_magnitude, force_variation, distance_scale, force_source, force_target, force_mode, drag, angular_drag })
    }
}

/// Rigidbody2D (class 50).
#[derive(Clone, Debug)]
pub struct Rigidbody2D {
    pub game_object: PPtr,
    pub body_type: i32,
    pub simulated: bool,
    pub use_full_kinematic_contacts: bool,
    pub use_auto_mass: bool,
    pub mass: f32,
    pub linear_drag: f32,
    pub angular_drag: f32,
    pub gravity_scale: f32,
    pub material: PPtr,
    pub interpolate: i32,
    pub sleeping_mode: i32,
    pub collision_detection: i32,
    pub constraints: i32,
}

impl Rigidbody2D {
    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        let game_object = PPtr::read(r)?;
        let body_type = r.i32()?;
        let (simulated, use_full_kinematic_contacts, use_auto_mass) = (r.bool()?, r.bool()?, r.bool()?);
        r.align();
        let v = Self {
            game_object,
            body_type,
            simulated,
            use_full_kinematic_contacts,
            use_auto_mass,
            mass: r.f32()?,
            linear_drag: r.f32()?,
            angular_drag: r.f32()?,
            gravity_scale: r.f32()?,
            material: PPtr::read(r)?,
            interpolate: r.i32()?,
            sleeping_mode: r.i32()?,
            collision_detection: r.i32()?,
            constraints: r.i32()?,
        };
        done(r, "Rigidbody2D", v)
    }
}

/// SpriteTilingProperty, inside Box/PolygonCollider2D.
#[derive(Clone, Debug)]
pub struct SpriteTiling {
    pub border: [f32; 4],
    pub pivot: [f32; 2],
    pub old_size: [f32; 2],
    pub new_size: [f32; 2],
    pub adaptive_tiling_threshold: f32,
    pub draw_mode: i32,
    pub adaptive_tiling: bool,
}

impl SpriteTiling {
    fn read(r: &mut Reader) -> Result<Self, String> {
        Ok(Self {
            border: floats(r)?,
            pivot: vec2(r)?,
            old_size: vec2(r)?,
            new_size: vec2(r)?,
            adaptive_tiling_threshold: r.f32()?,
            draw_mode: r.i32()?,
            adaptive_tiling: flag(r)?,
        })
    }
}

#[derive(Clone, Debug)]
pub enum ColliderShape {
    Box { tiling: SpriteTiling, auto_tiling: bool, size: [f32; 2], edge_radius: f32 },
    Circle { radius: f32 },
    Polygon { tiling: SpriteTiling, auto_tiling: bool, paths: Vec<Vec<[f32; 2]>> },
    Capsule { size: [f32; 2], direction: i32 },
    Edge { edge_radius: f32, points: Vec<[f32; 2]>, adjacent_start: Option<[f32; 2]>, adjacent_end: Option<[f32; 2]> },
}

/// Collider2D subclasses: CircleCollider2D (58), PolygonCollider2D (60), BoxCollider2D (61), EdgeCollider2D (68),
/// CapsuleCollider2D (70).
#[derive(Clone, Debug)]
pub struct Collider2D {
    pub game_object: PPtr,
    pub enabled: bool,
    pub density: f32,
    pub material: PPtr,
    pub is_trigger: bool,
    pub used_by_effector: bool,
    pub used_by_composite: bool,
    pub offset: [f32; 2],
    pub shape: ColliderShape,
}

impl Collider2D {
    pub const CLASSES: [i32; 5] = [58, 60, 61, 68, 70];

    pub fn read(class_id: i32, data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        let game_object = PPtr::read(r)?;
        let enabled = flag(r)?;
        let density = r.f32()?;
        let material = PPtr::read(r)?;
        let (is_trigger, used_by_effector, used_by_composite) = (r.bool()?, r.bool()?, r.bool()?);
        r.align();
        let offset = vec2(r)?;
        let shape = match class_id {
            58 => ColliderShape::Circle { radius: r.f32()? },
            61 => {
                let tiling = SpriteTiling::read(r)?;
                let auto_tiling = flag(r)?;
                ColliderShape::Box { tiling, auto_tiling, size: vec2(r)?, edge_radius: r.f32()? }
            }
            60 => {
                let tiling = SpriteTiling::read(r)?;
                let auto_tiling = flag(r)?;
                ColliderShape::Polygon { tiling, auto_tiling, paths: vec(r, |r| vec(r, vec2))? }
            }
            70 => ColliderShape::Capsule { size: vec2(r)?, direction: r.i32()? },
            68 => {
                let edge_radius = r.f32()?;
                let points = vec(r, vec2)?;
                let (start, end) = (vec2(r)?, vec2(r)?);
                let (use_start, use_end) = (r.bool()?, r.bool()?);
                r.align();
                ColliderShape::Edge {
                    edge_radius,
                    points,
                    adjacent_start: use_start.then_some(start),
                    adjacent_end: use_end.then_some(end),
                }
            }
            _ => return Err(format!("class {class_id} is not a Collider2D")),
        };
        let v = Self { game_object, enabled, density, material, is_trigger, used_by_effector, used_by_composite, offset, shape };
        done(r, "Collider2D", v)
    }
}

#[derive(Clone, Debug)]
pub enum JointKind {
    Hinge { use_motor: bool, motor_speed: f32, max_motor_torque: f32, use_limits: bool, lower: f32, upper: f32 },
    Fixed { damping_ratio: f32, frequency: f32 },
    Spring { auto_configure_distance: bool, distance: f32, damping_ratio: f32, frequency: f32 },
    Distance { auto_configure_distance: bool, distance: f32, max_distance_only: bool },
    Slider {
        auto_configure_angle: bool,
        angle: f32,
        use_motor: bool,
        motor_speed: f32,
        max_motor_force: f32,
        use_limits: bool,
        lower: f32,
        upper: f32,
    },
    Wheel { damping_ratio: f32, frequency: f32, angle: f32, use_motor: bool, motor_speed: f32, max_motor_torque: f32 },
}

/// AnchoredJoint2D subclasses: SpringJoint2D (231), DistanceJoint2D (232), HingeJoint2D (233), SliderJoint2D (234),
/// WheelJoint2D (235), FixedJoint2D (255).
#[derive(Clone, Debug)]
pub struct Joint2D {
    pub game_object: PPtr,
    pub enabled: bool,
    pub enable_collision: bool,
    pub connected_body: PPtr,
    pub break_force: f32,
    pub break_torque: f32,
    pub auto_configure_connected_anchor: bool,
    pub anchor: [f32; 2],
    pub connected_anchor: [f32; 2],
    pub kind: JointKind,
}

impl Joint2D {
    pub const CLASSES: [i32; 6] = [231, 232, 233, 234, 235, 255];

    pub fn read(class_id: i32, data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        let game_object = PPtr::read(r)?;
        let enabled = flag(r)?;
        let enable_collision = flag(r)?;
        let connected_body = PPtr::read(r)?;
        let (break_force, break_torque) = (r.f32()?, r.f32()?);
        let auto_configure_connected_anchor = flag(r)?;
        let (anchor, connected_anchor) = (vec2(r)?, vec2(r)?);
        let kind = match class_id {
            233 => {
                let use_motor = flag(r)?;
                let (motor_speed, max_motor_torque) = (r.f32()?, r.f32()?);
                JointKind::Hinge { use_motor, motor_speed, max_motor_torque, use_limits: flag(r)?, lower: r.f32()?, upper: r.f32()? }
            }
            255 => JointKind::Fixed { damping_ratio: r.f32()?, frequency: r.f32()? },
            231 => JointKind::Spring { auto_configure_distance: flag(r)?, distance: r.f32()?, damping_ratio: r.f32()?, frequency: r.f32()? },
            232 => JointKind::Distance { auto_configure_distance: flag(r)?, distance: r.f32()?, max_distance_only: flag(r)? },
            234 => {
                let (auto_configure_angle, angle) = (flag(r)?, r.f32()?);
                let use_motor = flag(r)?;
                let (motor_speed, max_motor_force) = (r.f32()?, r.f32()?);
                JointKind::Slider {
                    auto_configure_angle,
                    angle,
                    use_motor,
                    motor_speed,
                    max_motor_force,
                    use_limits: flag(r)?,
                    lower: r.f32()?,
                    upper: r.f32()?,
                }
            }
            235 => {
                let (damping_ratio, frequency, angle) = (r.f32()?, r.f32()?, r.f32()?);
                JointKind::Wheel { damping_ratio, frequency, angle, use_motor: flag(r)?, motor_speed: r.f32()?, max_motor_torque: r.f32()? }
            }
            _ => return Err(format!("class {class_id} is not an AnchoredJoint2D")),
        };
        let v = Self {
            game_object,
            enabled,
            enable_collision,
            connected_body,
            break_force,
            break_torque,
            auto_configure_connected_anchor,
            anchor,
            connected_anchor,
            kind,
        };
        done(r, "Joint2D", v)
    }
}

/// PhysicsMaterial2D (class 62).
#[derive(Clone, Debug)]
pub struct PhysicsMaterial2D {
    pub name: String,
    pub friction: f32,
    pub bounciness: f32,
}

impl PhysicsMaterial2D {
    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        let v = Self { name: r.string()?, friction: r.f32()?, bounciness: r.f32()? };
        done(r, "PhysicsMaterial2D", v)
    }
}

/// SpriteRenderer (class 212), with the Renderer fields scripts and drawing use.
#[derive(Clone, Debug)]
pub struct SpriteRenderer {
    pub game_object: PPtr,
    pub enabled: bool,
    pub materials: Vec<PPtr>,
    pub sorting_layer_id: i32,
    pub sorting_order: i16,
    pub sprite: PPtr,
    pub color: [f32; 4],
    pub flip_x: bool,
    pub flip_y: bool,
    pub draw_mode: i32,
    pub adaptive_mode_threshold: f32,
    /// SpriteTileMode: Continuous 0, Adaptive 1.
    pub sprite_tile_mode: i32,
    pub size: [f32; 2],
    pub mask_interaction: i32,
    pub sprite_sort_point: i32,
}

impl SpriteRenderer {
    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        let game_object = PPtr::read(r)?;
        let enabled = r.bool()?;
        // CastShadows, ReceiveShadows, DynamicOccludee, MotionVectors, LightProbeUsage, ReflectionProbeUsage,
        // RayTracingMode, RayTraceProcedural.
        r.bytes(8)?;
        r.align();
        let _rendering_layer_mask = r.u32()?;
        let _renderer_priority = r.i32()?;
        let _lightmap_index = (r.u16()?, r.u16()?);
        let _lightmap_tiling_offset: [[f32; 4]; 2] = [floats(r)?, floats(r)?];
        let materials = vec(r, PPtr::read)?;
        let _static_batch_info = (r.u16()?, r.u16()?);
        let _static_batch_root = PPtr::read(r)?;
        let _probe_anchor = PPtr::read(r)?;
        let _light_probe_volume_override = PPtr::read(r)?;
        let sorting_layer_id = r.i32()?;
        let _sorting_layer = r.i16()?;
        let sorting_order = r.i16()?;
        r.align();
        let sprite = PPtr::read(r)?;
        let color = floats(r)?;
        let (flip_x, flip_y) = (r.bool()?, r.bool()?);
        r.align();
        let draw_mode = r.i32()?;
        let size = vec2(r)?;
        let adaptive_mode_threshold = r.f32()?;
        let sprite_tile_mode = r.i32()?;
        let _was_sprite_assigned = flag(r)?;
        let v = Self {
            game_object,
            enabled,
            materials,
            sorting_layer_id,
            sorting_order,
            sprite,
            color,
            flip_x,
            flip_y,
            draw_mode,
            adaptive_mode_threshold,
            sprite_tile_mode,
            size,
            mask_interaction: r.i32()?,
            sprite_sort_point: r.i32()?,
        };
        done(r, "SpriteRenderer", v)
    }
}

/// Texture2D (class 28). Pixel data is inline or streamed from a .resS file.
#[derive(Clone, Debug)]
pub struct Texture2D {
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub complete_image_size: u32,
    pub format: i32,
    pub mip_count: i32,
    pub filter_mode: i32,
    pub wrap: [i32; 3],
    pub color_space: i32,
    pub image_data: Vec<u8>,
    /// (offset, size, path) into a .resS file when `image_data` is empty.
    pub stream: (u64, u32, String),
}

impl Texture2D {
    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        let name = r.string()?;
        let _forced_fallback_format = r.i32()?;
        let (_downscale_fallback, _is_alpha_channel_optional) = (r.bool()?, r.bool()?);
        r.align();
        let (width, height, complete_image_size) = (r.i32()?, r.i32()?, r.u32()?);
        let _mips_stripped = r.i32()?;
        let (format, mip_count) = (r.i32()?, r.i32()?);
        // IsReadable, IsPreProcessed, IgnoreMasterTextureLimit, StreamingMipmaps.
        r.bytes(4)?;
        let _streaming_mipmaps_priority = r.i32()?;
        let (_image_count, _texture_dimension) = (r.i32()?, r.i32()?);
        let filter_mode = r.i32()?;
        let (_aniso, _mip_bias) = (r.i32()?, r.f32()?);
        let wrap = [r.i32()?, r.i32()?, r.i32()?];
        let _lightmap_format = r.i32()?;
        let color_space = r.i32()?;
        let n = r.i32()? as usize;
        r.bytes(n)?;
        r.align();
        let n = r.i32()? as usize;
        let image_data = r.bytes(n)?.to_vec();
        r.align();
        let stream = (r.u64()?, r.u32()?, r.string()?);
        let v = Self { name, width, height, complete_image_size, format, mip_count, filter_mode, wrap, color_space, image_data, stream };
        done(r, "Texture2D", v)
    }

    /// Mip 0 as RGBA8 rows, bottom row first (Unity's order). `data_dir` holds the .resS files.
    pub fn rgba(&self, data_dir: &std::path::Path) -> Result<Vec<u8>, String> {
        if self.width == 0 || self.height == 0 {
            // Font textures that are filled at runtime.
            return Ok(Vec::new());
        }
        let streamed;
        let bytes = if self.image_data.is_empty() {
            let (offset, size, path) = &self.stream;
            if path.is_empty() {
                return Err(format!("{}: no image data ({}x{}, {} bytes, format {})", self.name, self.width, self.height, self.complete_image_size, self.format));
            }
            let file = data_dir.join(path.rsplit('/').next().unwrap_or(path));
            let all = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            streamed = all.get(*offset as usize..*offset as usize + *size as usize).ok_or("stream past the end")?.to_vec();
            &streamed
        } else {
            &self.image_data
        };
        let n = (self.width * self.height) as usize;
        let px = |bpp: usize| bytes.get(..n * bpp).ok_or_else(|| format!("{}: short image data", self.name));
        Ok(match self.format {
            1 => px(1)?.iter().flat_map(|&a| [0, 0, 0, a]).collect(),
            3 => px(3)?.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
            4 => px(4)?.to_vec(),
            5 => px(4)?.chunks(4).flat_map(|c| [c[1], c[2], c[3], c[0]]).collect(),
            7 => px(2)?
                .chunks(2)
                .flat_map(|c| {
                    let v = u16::from_le_bytes([c[0], c[1]]);
                    let (r, g, b) = ((v >> 11) as u8, (v >> 5 & 63) as u8, (v & 31) as u8);
                    [r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2, 255]
                })
                .collect(),
            17 => px(8)?
                .chunks(2)
                .map(|c| (half(u16::from_le_bytes([c[0], c[1]])).clamp(0.0, 1.0) * 255.0) as u8)
                .collect(),
            63 => px(1)?.iter().flat_map(|&v| [v, 0, 0, 255]).collect(),
            10 | 12 | 25 | 28 => {
                let (w, h) = (self.width as usize, self.height as usize);
                let mut out = vec![0u32; n];
                let res = match self.format {
                    10 => texture2ddecoder::decode_bc1(bytes, w, h, &mut out),
                    12 => texture2ddecoder::decode_bc3(bytes, w, h, &mut out),
                    25 => texture2ddecoder::decode_bc7(bytes, w, h, &mut out),
                    _ => texture2ddecoder::decode_unity_crunch(bytes, w, h, &mut out),
                };
                res.map_err(|e| format!("{}: {e}", self.name))?;
                // The decoders write BGRA.
                out.iter().flat_map(|p| {
                    let [b, g, r, a] = p.to_le_bytes();
                    [r, g, b, a]
                }).collect()
            }
            f => return Err(format!("{}: texture format {f} not rewritten", self.name)),
        })
    }
}

/// An IEEE half-precision float.
fn half(h: u16) -> f32 {
    let (sign, exp, frac) = ((h >> 15) as u32, (h >> 10 & 31) as u32, (h & 1023) as u32);
    let v = match exp {
        0 => frac as f32 / 1024.0 / 16384.0,
        31 => if frac == 0 { f32::INFINITY } else { f32::NAN },
        _ => f32::from_bits((exp + 112) << 23 | frac << 13),
    };
    if sign == 1 { -v } else { v }
}

#[derive(Clone, Debug)]
pub struct SubMesh {
    pub first_byte: u32,
    pub index_count: u32,
    pub topology: i32,
    pub base_vertex: u32,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub local_aabb: [f32; 6],
}

/// (stream, offset, format, dimension) of one vertex channel.
pub type Channel = [u8; 4];

/// Sprite (class 213).
#[derive(Clone, Debug)]
pub struct Sprite {
    pub name: String,
    pub rect: [f32; 4],
    pub offset: [f32; 2],
    pub border: [f32; 4],
    pub pixels_to_units: f32,
    pub pivot: [f32; 2],
    pub extrude: u32,
    pub is_polygon: bool,
    pub atlas: PPtr,
    pub texture: PPtr,
    pub alpha_texture: PPtr,
    pub sub_meshes: Vec<SubMesh>,
    pub index_buffer: Vec<u8>,
    pub vertex_count: u32,
    pub channels: Vec<Channel>,
    pub vertex_data: Vec<u8>,
    pub texture_rect: [f32; 4],
    pub texture_rect_offset: [f32; 2],
    pub atlas_rect_offset: [f32; 2],
    pub settings_raw: u32,
    pub uv_transform: [f32; 4],
    pub downscale_multiplier: f32,
    pub physics_shape: Vec<Vec<[f32; 2]>>,
}

impl Sprite {
    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        let name = r.string()?;
        let (rect, offset, border, pixels_to_units, pivot, extrude) =
            (floats(r)?, vec2(r)?, floats(r)?, r.f32()?, vec2(r)?, r.u32()?);
        let is_polygon = flag(r)?;
        let _render_data_key = (r.bytes(16)?, r.i64()?);
        let _atlas_tags = vec(r, |r| r.string())?;
        let atlas = PPtr::read(r)?;
        let texture = PPtr::read(r)?;
        let alpha_texture = PPtr::read(r)?;
        let _secondary_textures = vec(r, |r| Ok((PPtr::read(r)?, r.string()?)))?;
        let sub_meshes = vec(r, |r| {
            Ok(SubMesh {
                first_byte: r.u32()?,
                index_count: r.u32()?,
                topology: r.i32()?,
                base_vertex: r.u32()?,
                first_vertex: r.u32()?,
                vertex_count: r.u32()?,
                local_aabb: floats(r)?,
            })
        })?;
        let n = r.i32()? as usize;
        let index_buffer = r.bytes(n)?.to_vec();
        r.align();
        let vertex_count = r.u32()?;
        let channels = vec(r, |r| Ok([r.u8()?, r.u8()?, r.u8()?, r.u8()?]))?;
        let n = r.i32()? as usize;
        let vertex_data = r.bytes(n)?.to_vec();
        r.align();
        let _bindpose = vec(r, floats::<16>)?;
        let v = Self {
            name,
            rect,
            offset,
            border,
            pixels_to_units,
            pivot,
            extrude,
            is_polygon,
            atlas,
            texture,
            alpha_texture,
            sub_meshes,
            index_buffer,
            vertex_count,
            channels,
            vertex_data,
            texture_rect: floats(r)?,
            texture_rect_offset: vec2(r)?,
            atlas_rect_offset: vec2(r)?,
            settings_raw: r.u32()?,
            uv_transform: floats(r)?,
            downscale_multiplier: r.f32()?,
            physics_shape: vec(r, |r| vec(r, vec2))?,
        };
        let _bones = vec(r, |r| {
            let name = r.string()?;
            let (_position, _rotation): ([f32; 3], [f32; 4]) = (floats(r)?, floats(r)?);
            let (_length, _parent) = (r.f32()?, r.i32()?);
            Ok(name)
        })?;
        done(r, "Sprite", v)
    }
}

#[cfg(test)]
mod tests {
    use super::super::serialized::SerializedFile;
    use super::*;
    use std::collections::BTreeMap;

    /// Decoded pixels equal AssetRipper's PNG export of the same texture, for each texture format in the game.
    #[test]
    fn texture_pixels_match_export() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let export = std::path::Path::new("unity-export/ExportedProject/Assets/Texture2D");
        if !export.is_dir() {
            return;
        }
        let data = game.join("People Playground_Data");
        let mut checked = std::collections::BTreeMap::new();
        for e in std::fs::read_dir(&data).unwrap() {
            let name = e.unwrap().file_name().into_string().unwrap();
            if !(name.ends_with(".assets") || name.starts_with("level")) {
                continue;
            }
            let f = SerializedFile::open(&data.join(&name)).unwrap();
            for o in f.objects.iter().filter(|o| o.class_id == 28) {
                let t = Texture2D::read(f.data(o)).unwrap();
                let png_path = export.join(format!("{}.png", t.name));
                if checked.contains_key(&t.format) || t.width == 0 || !png_path.is_file() {
                    continue;
                }
                let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&png_path).unwrap()));
                let mut reader = decoder.read_info().unwrap();
                let mut buf = vec![0; reader.output_buffer_size().unwrap()];
                let info = reader.next_frame(&mut buf).unwrap();
                if (info.width, info.height) != (t.width as u32, t.height as u32) {
                    continue;
                }
                let buf: Vec<u8> = match info.color_type {
                    png::ColorType::Rgba => buf,
                    png::ColorType::Rgb => buf[..info.buffer_size()].chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
                    _ => continue,
                };
                let ours = t.rgba(&data).unwrap();
                let row = t.width as usize * 4;
                // PNG rows run top-down; Unity's bottom-up.
                let max_diff = (0..t.height as usize)
                    .flat_map(|y| {
                        let a = &ours[y * row..][..row];
                        let b = &buf[(t.height as usize - 1 - y) * row..][..row];
                        a.iter().zip(b).map(|(x, y)| x.abs_diff(*y))
                    })
                    .max()
                    .unwrap();
                checked.insert(t.format, (t.name.clone(), max_diff));
            }
        }
        eprintln!("{checked:?}");
        // RGB565 (7) is left out: the export shifts without bit replication, while the GPU (the only reader here;
        // "shard id mask" is not script-readable) replicates.
        for f in [1, 3, 4, 10, 12, 17, 25, 28, 63] {
            let (name, diff) = checked.get(&f).unwrap_or_else(|| panic!("no exported texture of format {f}"));
            assert_eq!(*diff, 0, "format {f} ({name})");
        }
    }

    /// Every physics/render component in the shipped data reads to exactly its serialized size.
    #[test]
    fn all_components_consume_exactly() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let data = game.join("People Playground_Data");
        let mut counts: BTreeMap<i32, (usize, Vec<String>)> = BTreeMap::new();
        let mut found_hinge = false;
        for e in std::fs::read_dir(&data).unwrap() {
            let name = e.unwrap().file_name().into_string().unwrap();
            if !(name.ends_with(".assets") || name.starts_with("level") || name == "globalgamemanagers") {
                continue;
            }
            let f = SerializedFile::open(&data.join(&name)).unwrap();
            for o in &f.objects {
                let d = f.data(o);
                let c = o.class_id;
                if c == 233 {
                    // A Person.prefab hinge, as the AssetRipper export of the same data shows it.
                    let j = Joint2D::read(c, d).unwrap();
                    if let JointKind::Hinge { max_motor_torque, use_limits, lower, upper, .. } = j.kind {
                        found_hinge |= j.anchor == [0.0032851696, -0.18]
                            && j.connected_anchor == [0.003785193, 0.11700004]
                            && j.break_force == f32::INFINITY
                            && (max_motor_torque, use_limits, lower, upper) == (1.0, true, -70.0, 80.0);
                    }
                }
                let res = match c {
                    50 => Rigidbody2D::read(d).map(drop),
                    62 => PhysicsMaterial2D::read(d).map(drop),
                    212 => SpriteRenderer::read(d).map(drop),
                    28 => Texture2D::read(d).and_then(|t| {
                        let px = t.rgba(&data)?;
                        if px.len() != (t.width * t.height * 4) as usize {
                            return Err("pixel count".into());
                        }
                        Ok(())
                    }),
                    213 => Sprite::read(d).map(drop),
                    c if Collider2D::CLASSES.contains(&c) => Collider2D::read(c, d).map(drop),
                    c if Joint2D::CLASSES.contains(&c) => Joint2D::read(c, d).map(drop),
                    _ => continue,
                };
                let e = counts.entry(c).or_default();
                e.0 += 1;
                if let Err(err) = res {
                    if e.1.len() < 3 {
                        e.1.push(format!("{name}#{}: {err}", o.path_id));
                    }
                }
            }
        }
        let mut bad = false;
        for (c, (n, errs)) in &counts {
            eprintln!("class {c}: {n} objects");
            for e in errs {
                eprintln!("    {e}");
                bad = true;
            }
        }
        assert!(!bad);
        assert!(found_hinge, "Person.prefab hinge (anchor 0.0032851696,-0.18, limits -70..80) not found");
        for c in [50, 61, 233, 212] {
            assert!(counts.contains_key(&c), "no class {c} objects");
        }
    }
}

/// Camera (class 20): the projection the simulation reads.
#[derive(Clone, Debug)]
pub struct Camera {
    pub orthographic: bool,
    pub orthographic_size: f32,
}

impl Camera {
    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        PPtr::read(r)?;
        flag(r)?; // m_Enabled
        r.i32()?; // m_ClearFlags
        floats::<4>(r)?; // m_BackGroundColor
        r.i32()?; // m_projectionMatrixMode
        r.i32()?; // m_GateFitMode (this build has no m_FOVAxisMode)
        floats::<5>(r)?; // m_SensorSize, m_LensShift, m_FocalLength
        floats::<7>(r)?; // m_NormalizedViewPortRect, near/far clip plane, field of view
        let orthographic = flag(r)?;
        let orthographic_size = r.f32()?;
        r.f32()?; // m_Depth
        r.i32()?; // m_CullingMask
        r.i32()?; // m_RenderingPath
        PPtr::read(r)?; // m_TargetTexture
        r.i32()?; // m_TargetDisplay
        r.i32()?; // m_TargetEye
        for _ in 0..4 {
            r.bool()?; // m_HDR, m_AllowMSAA, m_AllowDynamicResolution, m_ForceIntoRT
        }
        flag(r)?; // m_OcclusionCulling
        floats::<2>(r)?; // m_StereoConvergence, m_StereoSeparation
        done(r, "Camera", Self { orthographic, orthographic_size })
    }
}

/// The head of a ParticleSystem (class 198): its main module's timing (the modules after are not read).
#[derive(Clone, Copy, Debug)]
pub struct ParticleSystemHead {
    pub length_in_sec: f32,
    pub simulation_speed: f32,
    /// ParticleSystemStopAction: 0 None, 1 Disable, 2 Destroy, 3 Callback.
    pub stop_action: i32,
    pub looping: bool,
}

impl ParticleSystemHead {
    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        PPtr::read(r)?;
        let (length_in_sec, simulation_speed, stop_action) = (r.f32()?, r.f32()?, r.i32()?);
        r.i32()?; // cullingMode
        r.i32()?; // ringBufferMode
        vec2(r)?; // ringBufferLoopRange
        let looping = r.bool()?;
        Ok(Self { length_in_sec, simulation_speed, stop_action, looping })
    }
}

/// What the simulation reads of a TrailRenderer (class 96): the Renderer head as a SpriteRenderer's, then m_Time; the
/// rest (m_Parameters: width, curve, gradient) is rendering, and m_MinVertexDistance, m_Autodestruct and m_Emitting
/// are the last 6 bytes (a float and two bools).
#[derive(Clone, Copy, Debug)]
pub struct TrailHead {
    pub enabled: bool,
    pub time: f32,
    pub min_vertex_distance: f32,
    pub autodestruct: bool,
    pub emitting: bool,
}

impl TrailHead {
    pub fn read(data: &[u8]) -> Result<Self, String> {
        let r = &mut Reader::new(data);
        PPtr::read(r)?;
        let enabled = r.bool()?;
        r.bytes(8)?;
        r.align();
        r.u32()?; // m_RenderingLayerMask
        r.i32()?; // m_RendererPriority
        (r.u16()?, r.u16()?); // m_LightmapIndex
        let _: [[f32; 4]; 2] = [floats(r)?, floats(r)?];
        vec(r, PPtr::read)?; // m_Materials
        (r.u16()?, r.u16()?); // m_StaticBatchInfo
        for _ in 0..3 {
            PPtr::read(r)?; // m_StaticBatchRoot, m_ProbeAnchor, m_LightProbeVolumeOverride
        }
        r.i32()?; // m_SortingLayerID
        r.i16()?; // m_SortingLayer
        r.i16()?; // m_SortingOrder
        r.align();
        let time = r.f32()?;
        let n = data.len();
        if n < r.pos + 6 {
            return Err("TrailRenderer: no tail".into());
        }
        let tail = &data[n - 6..];
        if tail[4] > 1 || tail[5] > 1 {
            return Err("TrailRenderer: unexpected tail".into());
        }
        let min_vertex_distance = f32::from_le_bytes(tail[..4].try_into().unwrap());
        Ok(Self { enabled, time, min_vertex_distance, autodestruct: tail[4] == 1, emitting: tail[5] == 1 })
    }
}
