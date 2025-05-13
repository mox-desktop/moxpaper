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
    @location(6) alpha: f32,
    @location(7) radius: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) layer: u32,
    @location(2) size: vec2<f32>,
    @location(3) container_rect: vec4<f32>,
    @location(4) surface_position: vec2<f32>,
    @location(5) alpha: f32,
    @location(6) radius: f32,
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
    out.alpha = instance.alpha;
    let scaled_radius = instance.radius * instance.scale;
    let max_radius = min(scaled_size.x, scaled_size.y) * 0.5;
    out.radius = instance.radius;
    return out;
}

fn sdf_rounded_rect(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let radius_vec = vec4<f32>(r, r, r, r);
    var x = select(radius_vec.x, radius_vec.y, p.x > 0.0);
    var y = select(radius_vec.z, radius_vec.w, p.x > 0.0);
    let radius = select(y, x, p.y > 0.0);
    let q = abs(p) - b + radius;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
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

    let half_extent = in.size / 2.0;
    let p = (in.tex_coords - 0.5) * in.size;
    let d = sdf_rounded_rect(p, half_extent, in.radius);
    let aa = fwidth(d) * 0.6;
    let element_alpha = smoothstep(-aa, aa, -d);

    let local_pos_container = in.surface_position - vec2<f32>(in.container_rect.x, in.container_rect.y);
    let container_size = vec2<f32>(
        in.container_rect.z - in.container_rect.x,
        in.container_rect.w - in.container_rect.y
    );
    let half_extent_container = container_size / 2.0;
    let p_container = local_pos_container - half_extent_container;

    let target_container_radius = in.radius * 0.01 * length(half_extent_container);
    let eff_container_radius = min(target_container_radius, min(half_extent_container.x, half_extent_container.y));

    let container_dist = sdf_rounded_rect(p_container, half_extent_container, eff_container_radius);
    let container_aa = fwidth(container_dist) * 0.6;
    let container_alpha = smoothstep(-container_aa, container_aa, -container_dist);

    let final_alpha = tex_color.a * element_alpha * container_alpha * in.alpha;
    return vec4<f32>(tex_color.rgb, final_alpha);
}
