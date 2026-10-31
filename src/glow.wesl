// Unlit/AdditiveGlowSprite's fragment stage, disassembled: texture * vertex colour * _GlowColour * _GlowIntensity * vertex alpha.
// `color` holds all but the texture.
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> color: vec4<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var sprite: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var sprite_sampler: sampler;

@fragment
fn fragment(@location(2) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return textureSample(sprite, sprite_sampler, uv) * color;
}
