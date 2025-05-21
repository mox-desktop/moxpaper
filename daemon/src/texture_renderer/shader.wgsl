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
    @location(2) scale: f32,
    @location(3) opacity: f32,
    @location(4) rotation: f32,
    @location(5) rect: vec4<f32>,
    @location(6) radius: vec4<f32>,
    @location(7) container_rect: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) layer: u32,
    @location(1) opacity: f32,
    @location(2) rotation: f32,
    @location(3) tex_coords: vec2<f32>,
    @location(4) size: vec2<f32>,
    @location(5) surface_position: vec2<f32>,
    @location(6) radius: vec4<f32>,
    @location(7) container_rect: vec4<f32>,
    @location(8) screen_size: vec2<f32>,
};

fn rotation_matrix(angle: f32) -> mat2x2<f32> {
    let angle_inner = angle * pi / 180.0;
    let sinTheta = sin(angle_inner);
    let cosTheta = cos(angle_inner);
    return mat2x2<f32>(
        cosTheta, -sinTheta,
        sinTheta, cosTheta
    );
}

fn skew_matrix(skew_x: f32, skew_y: f32) -> mat2x2<f32> {
    return mat2x2<f32>(
        vec2<f32>(1.0, skew_y * pi / 180.0),
        vec2<f32>(skew_x * pi / 180.0, 1.0)
    );
}

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let pos = instance.rect.xy;
    let size = instance.rect.zw;

    let scaled_size = size * instance.scale;
    let local_pos = (model.position - vec2<f32>(0.5)) * scaled_size;
    let rotated_pos = rotation_matrix(instance.rotation) * local_pos;
    let position = rotated_pos + pos + scaled_size * 0.5;

    out.clip_position = vec4<f32>(
        2.0 * vec2<f32>(position) / vec2<f32>(params.screen_resolution) - 1.0,
        0.0,
        1.0,
    );
    out.tex_coords = model.position;
    out.layer = instance_idx;
    out.size = scaled_size;
    out.container_rect = instance.container_rect;
    out.surface_position = position;
    out.opacity = instance.opacity;
    out.rotation = instance.rotation;
    out.radius = instance.radius;
    out.screen_size = vec2<f32>(params.screen_resolution);

    return out;
}

fn sdf_rounded_rect(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    var x = select(r.x, r.y, p.x > 0.0);
    var y = select(r.z, r.w, p.x > 0.0);
    let radius = select(y, x, p.y > 0.0);
    let q = abs(p) - b + radius;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>; 
@group(0) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_coords = vec2<f32>(in.tex_coords.x, 1.0 - in.tex_coords.y);
    let base_color = textureSample(t_diffuse, s_diffuse, tex_coords);
  
    // === TEXTURE ROUNDED CORNERS HANDLING ===
    let centered_tex_coords = in.tex_coords - 0.5;
    let half_extent = vec2<f32>(0.5, 0.5);
    let texture_radius = in.radius * 0.01;
    let max_radius = vec4<f32>(half_extent.x, half_extent.x, half_extent.y, half_extent.y);
    let effective_radius = min(texture_radius, max_radius);
    let texture_dist = sdf_rounded_rect(centered_tex_coords, half_extent, effective_radius);
    let texture_aa = fwidth(texture_dist) * 0.6;
    let texture_alpha = smoothstep(-texture_aa, texture_aa, -texture_dist);
    
    // === CONTAINER CLIPPING HANDLING ===
    let container_center = vec2<f32>(
        (in.container_rect.x + in.container_rect.z) / 2.0,
        (in.container_rect.y + in.container_rect.w) / 2.0
    );
    let local_pos_container = in.surface_position - container_center;
    let rotated_local_pos = rotation_matrix(-in.rotation) * local_pos_container;
    let container_size = vec2<f32>(
        in.container_rect.z - in.container_rect.x,
        in.container_rect.w - in.container_rect.y
    );
    let half_extent_container = container_size / 2.0;
    let d = abs(rotated_local_pos) - half_extent_container;
    let container_dist = length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
    let container_aa = fwidth(container_dist) * 0.6;
    let container_alpha = smoothstep(-container_aa, container_aa, -container_dist);

    return vec4<f32>(base_color.rgb, base_color.a * texture_alpha * container_alpha * in.opacity);
}
