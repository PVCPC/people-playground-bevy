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
        let _adaptive_mode_threshold = r.f32()?;
        let _sprite_tile_mode = r.i32()?;
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
            size,
            mask_interaction: r.i32()?,
            sprite_sort_point: r.i32()?,
        };
        done(r, "SpriteRenderer", v)
    }
}

#[cfg(test)]
mod tests {
    use super::super::serialized::SerializedFile;
    use super::*;
    use std::collections::BTreeMap;

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
