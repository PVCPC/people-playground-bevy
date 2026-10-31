//! The game's cameras: Main Camera draws the scene, LightsCamera draws the LightSprite layer (half size, additive) into a
//! texture, and LightsEffect multiplies the two (Main.unity's cameras, Hidden/Custom/LightPost).

use bevy::{
    asset::embedded_asset,
    camera::{visibility::RenderLayers, ClearColorConfig, Hdr, ImageRenderTarget, RenderTarget, ScalingMode},
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    mesh::MeshVertexBufferLayoutRef,
    prelude::*,
    render::{
        render_resource::{AsBindGroup, ShaderType, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d, RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureFormat},

    },
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin},
    window::PrimaryWindow,
};

/// GameObject layer LightSprite (LightsCamera's cullingMask).
const LIGHT_SPRITE: i32 = 15;
/// GameObject layer ScreenUI (On screen UI camera's cullingMask).
const SCREEN_UI: i32 = 16;
/// Main Camera's cullingMask.
const MAIN_MASK: u32 = 12087;
/// LightsEffect in DEFAULT POST PROCESSING PROFILE.
const INTENSITY: f32 = 1.0;
const MINIMUM: f32 = 0.025;
const MAXIMUM: f32 = 100.0;

/// The camera the game's Main Camera is: the world view scripts and the mouse go through.
#[derive(Component)]
pub struct MainCamera;

#[derive(Component)]
struct LightsCamera;

#[derive(Resource)]
struct Targets {
    scene: Handle<Image>,
    lights: Handle<Image>,
}

/// Unlit/AdditiveGlowSprite. `color` is everything but the texture.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct Glow {
    #[uniform(0)]
    pub color: Vec4,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Handle<Image>,
}

impl Material2d for Glow {
    fn fragment_shader() -> ShaderRef {
        "embedded://people_playground_bevy/glow.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(_: &bevy::sprite_render::Material2dPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: Material2dKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        // Blend One One.
        let add = BlendComponent { src_factor: BlendFactor::One, dst_factor: BlendFactor::One, operation: BlendOperation::Add };
        if let Some(Some(target)) = descriptor.fragment.as_mut().and_then(|f| f.targets.first_mut()) {
            target.blend = Some(BlendState { color: add, alpha: add });
        }
        Ok(())
    }
}

/// Unlit/Jaap/PanelsShader's properties.
#[derive(ShaderType, Clone)]
pub struct PanelSettings {
    pub offset: Vec2,
    pub scrolling_speed: Vec2,
    pub fade: Vec4,
    pub panels: Vec2,
    pub scale: f32,
    pub colour_variation: f32,
    pub alpha: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct Panels {
    #[uniform(0)]
    pub settings: PanelSettings,
    #[texture(1)]
    #[sampler(2)]
    pub main: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub panels: Handle<Image>,
}

impl Material2d for Panels {
    fn fragment_shader() -> ShaderRef {
        "embedded://people_playground_bevy/panels.wesl".into()
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct Composite {
    #[uniform(0)]
    settings: Vec3,
    #[texture(1)]
    #[sampler(2)]
    scene: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    lights: Handle<Image>,
}

impl Material2d for Composite {
    fn fragment_shader() -> ShaderRef {
        "embedded://people_playground_bevy/composite.wesl".into()
    }
}

/// Which camera draws a GameObject layer: 0 Main, 1 Lights, 2 the final view (nothing else is lit), 3 none.
pub fn camera_layers(game_object_layer: i32) -> RenderLayers {
    RenderLayers::layer(match game_object_layer {
        LIGHT_SPRITE => 1,
        SCREEN_UI => 2,
        l if MAIN_MASK >> l & 1 == 1 => 0,
        _ => 3,
    })
}

pub fn add(app: &mut App) {
    embedded_asset!(app, "glow.wesl");
    embedded_asset!(app, "composite.wesl");
    embedded_asset!(app, "panels.wesl");
    app.add_plugins((Material2dPlugin::<Glow>::default(), Material2dPlugin::<Composite>::default(), Material2dPlugin::<Panels>::default()))
        .add_systems(Startup, setup)
        .add_systems(PostUpdate, follow.before(TransformSystems::Propagate));
}

/// Main Camera's orthographic size in the scene: 5 up and down from the middle.
pub fn start_projection() -> Projection {
    Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::FixedVertical { viewport_height: 2.0 }, scale: 5.0, ..OrthographicProjection::default_2d() })
}

fn target(handle: &Handle<Image>, scale_factor: f32) -> RenderTarget {
    RenderTarget::Image(ImageRenderTarget { handle: handle.clone(), scale_factor })
}

fn setup(
    mut commands: Commands,
    window: Single<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut composites: ResMut<Assets<Composite>>,
) {
    let (w, h, factor) = (window.physical_width().max(1), window.physical_height().max(1), window.scale_factor());
    let image = |w, h| Image::new_target_texture(w, h, TextureFormat::Rgba16Float, None);
    let scene = images.add(image(w, h));
    let lights = images.add(image((w / 2).max(1), (h / 2).max(1)));
    let black = ClearColorConfig::Custom(Color::BLACK);
    commands.spawn((
        MainCamera,
        Camera2d,
        Camera { order: -2, clear_color: black.clone(), ..default() },
        target(&scene, factor),
        Hdr,
        Tonemapping::None,
        DebandDither::Disabled,
        start_projection(),
    ));
    commands.spawn((
        LightsCamera,
        Camera2d,
        Camera { order: -1, clear_color: black.clone(), ..default() },
        target(&lights, factor / 2.0),
        Hdr,
        Tonemapping::None,
        DebandDither::Disabled,
        RenderLayers::layer(1),
    ));
    // The post stack's last step, onto the window; the UI draws over it.
    commands.spawn((
        Camera2d,
        Camera { order: 0, clear_color: black, ..default() },
        Hdr,
        Tonemapping::None,
        RenderLayers::layer(2),
        IsDefaultUiCamera,
        Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::Fixed { width: 1.0, height: 1.0 }, ..OrthographicProjection::default_2d() }),
    ));
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial2d(composites.add(Composite { settings: Vec3::new(INTENSITY, MINIMUM, MAXIMUM), scene: scene.clone(), lights: lights.clone() })),
        RenderLayers::layer(2),
    ));
    commands.insert_resource(Targets { scene, lights });
}

/// LightRenderCameraBehaviour: follows Main Camera; both textures follow the window (ResetRT).
fn follow(
    window: Single<&Window, With<PrimaryWindow>>,
    targets: Res<Targets>,
    mut images: ResMut<Assets<Image>>,
    main: Single<(&Transform, &Projection), (With<MainCamera>, Without<LightsCamera>)>,
    mut lights: Single<(&mut Transform, &mut Projection), With<LightsCamera>>,
    mut scales: Query<(&mut RenderTarget, Has<LightsCamera>), Or<(With<MainCamera>, With<LightsCamera>)>>,
) {
    *lights.0 = *main.0;
    *lights.1 = main.1.clone();
    let (w, h) = (window.physical_width().max(1), window.physical_height().max(1));
    if images.get(&targets.scene).is_some_and(|i| i.size() == UVec2::new(w, h)) {
        return;
    }
    let size = |w, h| Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    if let Some(mut i) = images.get_mut(&targets.scene) {
        i.resize(size(w, h));
    }
    if let Some(mut i) = images.get_mut(&targets.lights) {
        i.resize(size((w / 2).max(1), (h / 2).max(1)));
    }
    for (mut render_target, light) in &mut scales {
        *render_target = target(if light { &targets.lights } else { &targets.scene }, if light { window.scale_factor() / 2.0 } else { window.scale_factor() });
    }
}
