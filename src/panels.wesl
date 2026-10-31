// Unlit/Jaap/PanelsShader, disassembled: a random panel of the atlas for each world cell, fading to _FadeColour as the view widens.
import bevy_sprite_render::mesh2d::{vertex_output::VertexOutput, view_bindings::{view, globals}};

struct Settings {
    offset: vec2<f32>,
    scrolling_speed: vec2<f32>,
    fade: vec4<f32>,
    /// The atlas's width and height in panels.
    panels: vec2<f32>,
    scale: f32,
    colour_variation: f32,
    /// The vertex colour's alpha.
    alpha: f32,
}
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> settings: Settings;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var main_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var main_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var panel_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var panel_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if settings.alpha < 0.5 {
        return settings.fade;
    }
    let p = settings.scale * (in.world_position.xy - settings.offset) + settings.scrolling_speed * globals.time;
    // unity_OrthoParams.x, half the view's width.
    let fade = saturate(0.015 / view.clip_from_view[0][0]);
    let cell = ceil(p);
    var h = fract(vec3<f32>(cell.x, cell.y, cell.x) * vec3<f32>(3.157385, 3.342350, 3.345595));
    h += dot(h.zyx, h.yzx + vec3<f32>(33.33));
    h = fract(vec3<f32>(h.x * (h.y + h.z), h.y * (h.x + h.z), h.z * (h.x + h.y)));
    let n = 1.0 / settings.panels.x;
    // fmod(p, 1): keeps p's sign, as frac(abs) * sign does.
    let s = select(vec2<f32>(-1.0), vec2<f32>(1.0), p >= -p);
    let local = fract(s * p) * s / settings.panels;
    let uv = ceil(h.xy / n) * n + local;
    let panel = textureSample(panel_texture, panel_sampler, uv) - vec4<f32>(h.z * settings.colour_variation);
    let main = textureSample(main_texture, main_sampler, local);
    return mix(main * panel, settings.fade, fade);
}
