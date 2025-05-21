const pi = radians(180.0);

struct Params {
    screen_resolution: vec2<u32>,
    _pad: vec2<u32>,
};
@group(1) @binding(0)
var<uniform> params: Params;

struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct InstanceInput {
    @location(2) blur_sigma: u32,
    @location(3) blur_color: vec4<f32>,
    @location(4) rect: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) blur_sigma: u32,
    @location(1) tex_coords: vec2<f32>,
    @location(2) screen_size: vec2<f32>,
    @location(3) blur_color: vec4<f32>,
};

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let pos = instance.rect.xy;
    let size = instance.rect.zw;

    let local_pos = (model.position - vec2<f32>(0.5)) * size;
    let position = local_pos + pos + size * 0.5;

    out.clip_position = vec4<f32>(
        2.0 * vec2<f32>(position) / vec2<f32>(params.screen_resolution) - 1.0,
        0.0,
        1.0,
    );
    out.tex_coords = model.position;
    out.screen_size = vec2<f32>(params.screen_resolution);
    out.blur_sigma = instance.blur_sigma;
    out.blur_color = instance.blur_color;

    return out;
}

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>; 
@group(0) @binding(1)
var s_diffuse: sampler;
@group(0) @binding(2)
var<storage, read> weights: array<f32>;
@group(0) @binding(3)
var<storage, read> offsets: array<f32>;

@fragment
fn fs_horizontal_blur(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_coords = vec2<f32>(in.tex_coords.x, 1.0 - in.tex_coords.y);

    if in.blur_sigma == 0 {
        return textureSample(t_diffuse, s_diffuse, tex_coords);
    }

    var color: vec4<f32> = in.blur_color;
    for (var i: u32 = 0; i < in.blur_sigma * 3; i++) {
        let offset = offsets[i];
        let weight = weights[i];
        let tex_offset = vec2<f32>(offset / in.screen_size.x, 0.0);
        let sample_coord = tex_coords + tex_offset;
        color += textureSample(t_diffuse, s_diffuse, sample_coord) * weight;
    }

    return color;
}

@fragment
fn fs_vertical_blur(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_coords = vec2<f32>(in.tex_coords.x, 1.0 - in.tex_coords.y);

    if in.blur_sigma == 0 {
        return textureSample(t_diffuse, s_diffuse, tex_coords);
    }

    var color: vec4<f32> = in.blur_color;
    for (var i: u32 = 0; i < in.blur_sigma * 3; i++) {
        let offset = offsets[i];
        let weight = weights[i];
        let tex_offset = vec2<f32>(0.0, offset / in.screen_size.y);
        let sample_coord = tex_coords + tex_offset;
        color += textureSample(t_diffuse, s_diffuse, sample_coord) * weight;
    }

    return color;
}
