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
    @location(1) scale: f32,
    @location(2) opacity: f32,
    @location(3) rotation: f32,
    @location(4) brightness: f32,
    @location(5) contrast: f32,
    @location(6) saturation: f32,
    @location(7) hue_rotate: f32,
    @location(8) sepia: f32,
    @location(9) invert: f32,
    @location(10) grayscale: f32,
    @location(11) skew: vec2<f32>,
    @location(12) rect: vec4<f32>,
    @location(13) radius: vec4<f32>,
    @location(14) container_rect: vec4<f32>,
    @location(15) shadow: vec3<f32>,
};

struct VertexOutput {
    @location(0) layer: u32,
    @location(1) opacity: f32,
    @location(2) rotation: f32,
    @location(3) brightness: f32,
    @location(4) contrast: f32,
    @location(5) saturation: f32,
    @location(6) hue_rotate: f32,
    @location(7) sepia: f32,
    @location(8) invert: f32,
    @location(9) grayscale: f32,
    @location(10) tex_coords: vec2<f32>,
    @location(11) size: vec2<f32>,
    @location(12) surface_position: vec2<f32>,
    @location(13) screen_size: vec2<f32>,
    @location(14) shadow_softness: f32,
    @location(15) shadow_offset: vec2<f32>,
    @location(16) radius: vec4<f32>,
    @location(17) container_rect: vec4<f32>,
    @builtin(position) clip_position: vec4<f32>,
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

    let pos = instance.rect.xy * instance.scale;
    let size = instance.rect.zw * instance.scale;

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
    out.size = size;
    out.container_rect = instance.container_rect;
    out.surface_position = position;
    out.opacity = instance.opacity;
    out.rotation = instance.rotation;
    out.radius = instance.radius;
    out.brightness = instance.brightness;
    out.contrast = instance.contrast;
    out.saturation = instance.saturation;
    out.hue_rotate = instance.hue_rotate;
    out.sepia = instance.sepia;
    out.invert = instance.invert;
    out.grayscale = instance.grayscale;
    out.screen_size = vec2<f32>(params.screen_resolution);
    out.shadow_offset = instance.shadow.xy;
    out.shadow_softness = instance.shadow.z;

    return out;
}

fn sdf_rounded_rect(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    var x = select(r.x, r.y, p.x > 0.0);
    var y = select(r.z, r.w, p.x > 0.0);
    let radius = select(y, x, p.y > 0.0);
    let q = abs(p) - b + radius;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

fn brightness_matrix(brightness: f32) -> mat4x4<f32> {
    return mat4x4<f32>(
        1, 0, 0, 0,
        0, 1, 0, 0,
        0, 0, 1, 0,
        brightness, brightness, brightness, 1
    );
}

fn contrast_matrix(contrast: f32) -> mat4x4<f32> {
    let t = (1.0 - contrast) / 2.0;
    return mat4x4<f32>(
        contrast, 0, 0, 0,
        0, contrast, 0, 0,
        0, 0, contrast, 0,
        t, t, t, 1
    );
}

fn saturation_matrix(saturation: f32) -> mat4x4<f32> {
    let luminance = vec3<f32>(0.3086, 0.6094, 0.0820);
    let one_minus_sat = 1.0 - saturation;

    var red: vec3<f32> = vec3<f32>(luminance.x * one_minus_sat);
    red += vec3<f32>(saturation, 0, 0);

    var green: vec3<f32> = vec3<f32>(luminance.y * one_minus_sat);
    green += vec3<f32>(0, saturation, 0);

    var blue: vec3<f32> = vec3<f32>(luminance.z * one_minus_sat);
    blue += vec3<f32>(0, 0, saturation);

    return mat4x4<f32>(
        vec4<f32>(red, 0.0),
        vec4<f32>(green, 0.0),
        vec4<f32>(blue, 0.0),
        vec4<f32>(0.0, 0.0, 0.0, 1.0)
    );
}

fn grayscale(color: vec3<f32>, intensity: f32) -> vec3<f32> {
    let luminance = dot(color, vec3<f32>(0.299, 0.587, 0.114));
    return mix(color, vec3<f32>(luminance), intensity);
}

fn sepia(color: vec3<f32>, sepia: f32) -> vec3<f32> {
    let sepia_matrix = vec3<f32>(
        dot(color.rgb, vec3<f32>(0.393, 0.769, 0.189)),
        dot(color.rgb, vec3<f32>(0.349, 0.686, 0.168)),
        dot(color.rgb, vec3<f32>(0.272, 0.534, 0.131))
    );
    return mix(color.rgb, sepia_matrix, sepia);
}

fn hue_rotate(color: vec3<f32>, angle: f32) -> vec3<f32> {
    return vec3<f32>(
        dot(color, vec3<f32>(0.213, 0.715, -0.213)) * (1.0 - cos(angle)) + cos(angle) * color.r + sin(angle) * color.b,
        dot(color, vec3<f32>(-0.213, 0.715, 0.715)) * (1.0 - cos(angle)) + cos(angle) * color.g + sin(angle) * color.g,
        dot(color, vec3<f32>(0.272, -0.715, 0.213)) * (1.0 - cos(angle)) + cos(angle) * color.b + sin(angle) * color.r
    );
}

@group(0) @binding(0)
var t_diffuse: texture_2d_array<f32>; 
@group(0) @binding(1)
var s_diffuse: sampler;

fn gaussian_shadow(dist: f32, blur_radius: f32) -> f32 {
    if blur_radius <= 0.0 {
        return select(0.0, 1.0, dist <= 0.0);
    }
    
    // Normalize distance by blur radius
    let normalized_dist = abs(dist) / blur_radius;
    
    // Gaussian approximation
    let gaussian = exp(-normalized_dist * normalized_dist * 0.5);
    
    // Smooth falloff for distances beyond the blur radius
    let falloff = smoothstep(0.0, 1.0, 1.0 - normalized_dist);

    return gaussian * falloff;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_coords = vec2<f32>(in.tex_coords.x, 1.0 - in.tex_coords.y);
    let base_color = textureSample(t_diffuse, s_diffuse, tex_coords, in.layer);
  
    // === TEXTURE ROUNDED CORNERS HANDLING ===
    let centered_tex_coords = in.tex_coords - 0.5;
    let half_extent = vec2<f32>(0.5, 0.5);
    let texture_radius = in.radius * 0.01;
    let max_radius = vec4<f32>(half_extent.x, half_extent.x, half_extent.y, half_extent.y);
    let effective_radius = min(texture_radius, max_radius);
    let texture_dist = sdf_rounded_rect(centered_tex_coords, half_extent, effective_radius);
    let texture_aa = fwidth(texture_dist) * 0.6;
    let texture_alpha = smoothstep(-texture_aa, texture_aa, -texture_dist);

    // === SHADOW HANDLING ===
    let shadow_offset_normalized = in.shadow_offset / in.size;
    let shadow_coords = centered_tex_coords - shadow_offset_normalized;
    let shadow_dist = sdf_rounded_rect(shadow_coords, half_extent + (in.shadow_offset / in.size) / 2., effective_radius);
    let shadow_alpha = gaussian_shadow(shadow_dist, in.shadow_softness / min(in.size.x, in.size.y));
    
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

    // === FINAL COLOR COMPOSITION ===
    var main_color = vec4<f32>(base_color.rgb, base_color.a * texture_alpha * container_alpha * in.opacity);
    main_color = brightness_matrix(in.brightness) * contrast_matrix(in.contrast) * saturation_matrix(in.saturation) * main_color;

    let hue_rotated = hue_rotate(main_color.rgb, in.hue_rotate);
    let sepia_applied = sepia(hue_rotated, in.sepia);
    let gray_applied = grayscale(sepia_applied, in.grayscale);
    let final_main_color = vec4<f32>(mix(gray_applied, vec3<f32>(1.0) - gray_applied, in.invert), main_color.a);

    //return vec4<f32>(shadow_alpha);

    let shadow_contribution = vec4<f32>(0.0, 0.0, 0.0, shadow_alpha) * (1.0 - final_main_color.a);
    let final_color = final_main_color + shadow_contribution;
    return final_color;
}
