// Hidden/Custom/LightPost's fragment stage, disassembled: the scene lit by LightsCamera's texture, near-black fading to grey.
struct Settings { intensity: f32, minimum: f32, maximum: f32 }
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> settings: Settings;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var scene: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var scene_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var lights: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var lights_sampler: sampler;

@fragment
fn fragment(@location(2) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let light = min(textureSample(lights, lights_sampler, uv) * settings.intensity, vec4<f32>(settings.maximum)) + vec4<f32>(settings.minimum);
    let colour = textureSample(scene, scene_sampler, uv);
    let lit = light * colour;
    let grey = dot(lit.rgb, vec3<f32>(0.299, 0.587, 0.114));
    return vec4<f32>(saturate(grey * 25.0) * (colour.rgb * light.rgb - vec3<f32>(grey)) + vec3<f32>(grey), lit.a);
}
