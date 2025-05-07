struct ProjectionUniform {
    view_proj: mat4x4<f32>,
};
@group(1) @binding(0)
var<uniform> projection: ProjectionUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct InstanceInput {
    @location(2) pos: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) container_rect: vec4<f32>,
    @location(5) scale: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) layer: u32,
    @location(2) size: vec2<f32>,
    @location(3) container_rect: vec4<f32>,
    @location(4) surface_position: vec2<f32>,
};

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let scaled_size = instance.size * instance.scale;
    let position = model.position * scaled_size + instance.pos;

    out.clip_position = projection.view_proj * vec4<f32>(position, 0.0, 1.0);
    out.tex_coords = model.position;
    out.layer = instance_idx;

    out.size = scaled_size;
    out.container_rect = instance.container_rect;
    out.surface_position = position;

    return out;
}

@group(0) @binding(0)
var t_diffuse: texture_2d_array<f32>; 
@group(0) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(
        t_diffuse,
        s_diffuse,
        vec2<f32>(in.tex_coords.x, 1.0 - in.tex_coords.y),
        i32(in.layer)
    );

    return tex_color;
}

