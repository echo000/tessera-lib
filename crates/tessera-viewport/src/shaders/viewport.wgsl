// Tessera viewport shader: skinned meshes, lines (grid/bones) and 2d images.
//
// Matrices are uploaded in porter-math memory order, which WGSL reads as the
// transpose of the row-vector convention porter-math uses on the cpu; `M * v`
// here therefore matches `v.transform(&M)` there.

struct Camera {
    // Projection * view.
    view_proj: mat4x4<f32>,
    // Inverse of projection * rotation-only view: pixel to eye-relative ray.
    grid_inverse: mat4x4<f32>,
    // Projection * rotation-only view: eye-relative point to clip space.
    grid_view: mat4x4<f32>,
    // Up-axis correction for the previewed asset. Pure rotation, so it is
    // used for normals as well.
    model: mat4x4<f32>,
    // The camera position in world space.
    eye: vec4<f32>,
    // x: eye height, y: minor spacing, z: major spacing, w: fade distance.
    params: vec4<f32>,
    // Eye xz modulo the minor (xy) and major (zw) spacing.
    grid_phase: vec4<f32>,
    // xy: world origin relative to the eye; zw: x / z axis visibility.
    grid_axes: vec4<f32>,
    // x: default shaded, y: srgb, z: grayscale image.
    flags: vec4<u32>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var t_albedo: texture_2d<f32>;
@group(1) @binding(1)
var s_albedo: sampler;

struct Material {
    tint: vec4<f32>,
    flags: vec4<u32>,
    // Strength bits, encoding (RGB=0/RG=1), invert Y, enabled.
    normal_map: vec4<u32>,
}
@group(1) @binding(2)
var<uniform> material: Material;
@group(1) @binding(3)
var t_normal: texture_2d<f32>;

// Skinning matrices: world * inverse bind pose per bone. A single identity
// matrix when the preview is not animated.
@group(2) @binding(0)
var<storage, read> bones: array<mat4x4<f32>>;

fn linear_to_srgb(linear: vec3<f32>) -> vec3<f32> {
    let less = linear * 12.92;
    let more = pow(max(linear, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) * 1.055 - vec3<f32>(0.055);
    return select(less, more, linear > vec3<f32>(0.0031308));
}

// -- Meshes -----------------------------------------------------------------

struct MeshInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) bone_ids: vec4<u32>,
    @location(4) bone_weights: vec4<f32>,
    @location(5) color: vec4<f32>,
    @location(10) bone_ids_2: vec4<u32>,
    @location(11) bone_weights_2: vec4<f32>,
}

// Per-instance placement (T * R * S), split into columns. Model previews draw
// a single identity instance; world previews draw one per placement.
struct MeshInstance {
    @location(6) placement_0: vec4<f32>,
    @location(7) placement_1: vec4<f32>,
    @location(8) placement_2: vec4<f32>,
    @location(9) placement_3: vec4<f32>,
}

struct MeshOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
}

@vertex
fn vs_mesh(in: MeshInput, instance: MeshInstance) -> MeshOutput {
    let bone_count = arrayLength(&bones);
    let weights = array<vec4<f32>, 2>(in.bone_weights, in.bone_weights_2);
    let ids = array<vec4<u32>, 2>(in.bone_ids, in.bone_ids_2);
    let total = dot(in.bone_weights + in.bone_weights_2, vec4<f32>(1.0));

    var position = in.position;
    var normal = in.normal;

    if total > 0.0001 {
        var skinned_position = vec3<f32>(0.0);
        var skinned_normal = vec3<f32>(0.0);

        for (var i = 0u; i < 8u; i += 1u) {
            let weight = weights[i / 4u][i % 4u] / total;

            if weight > 0.0 {
                let transform = bones[min(ids[i / 4u][i % 4u], bone_count - 1u)];

                skinned_position += (transform * vec4<f32>(in.position, 1.0)).xyz * weight;
                skinned_normal += (transform * vec4<f32>(in.normal, 0.0)).xyz * weight;
            }
        }

        position = skinned_position;
        normal = skinned_normal;
    }

    let placement = mat4x4<f32>(
        instance.placement_0,
        instance.placement_1,
        instance.placement_2,
        instance.placement_3,
    );

    // Inverse transpose of a T * R * S matrix: each basis column is the rotated
    // axis times its scale, so dividing by the squared length leaves axis / scale.
    let axis_x = placement[0].xyz;
    let axis_y = placement[1].xyz;
    let axis_z = placement[2].xyz;

    let normal_placement = mat3x3<f32>(
        axis_x / max(dot(axis_x, axis_x), 1e-8),
        axis_y / max(dot(axis_y, axis_y), 1e-8),
        axis_z / max(dot(axis_z, axis_z), 1e-8),
    );

    var out: MeshOutput;

    let world_position = camera.model * placement * vec4<f32>(position, 1.0);

    out.position = camera.view_proj * world_position;
    out.world_position = world_position.xyz;
    // camera.model is a pure rotation, valid for normals as-is.
    out.normal = (camera.model * vec4<f32>(normal_placement * normal, 0.0)).xyz;
    out.uv = in.uv;
    out.color = in.color;

    return out;
}

fn mesh_normal(in: MeshOutput) -> vec3<f32> {
    let normal = normalize(in.normal);
    let position_dx = dpdx(in.world_position);
    let position_dy = dpdy(in.world_position);
    let uv_dx = dpdx(in.uv);
    let uv_dy = dpdy(in.uv);

    if material.normal_map.w == 0u {
        return normal;
    }

    // Reconstruct after filtering XY, matching engines which store two channels.
    let sample = textureSampleGrad(t_normal, s_albedo, in.uv, uv_dx, uv_dy).xyz * 2.0 - 1.0;
    var xy = sample.xy * bitcast<f32>(material.normal_map.x);
    var z = sample.z;
    if material.normal_map.y == 1u {
        z = sqrt(max(1.0 - dot(xy, xy), 0.0));
    }
    if material.normal_map.z != 0u {
        xy.y = -xy.y;
    }

    // Derivative cotangent frame also preserves mirrored UV handedness.
    let position_dy_perp = cross(position_dy, normal);
    let position_dx_perp = cross(normal, position_dx);
    let tangent = position_dy_perp * uv_dx.x + position_dx_perp * uv_dy.x;
    let bitangent = position_dy_perp * uv_dx.y + position_dx_perp * uv_dy.y;
    let t = tangent * inverseSqrt(dot(tangent, tangent) + 1e-30);
    let b = bitangent * inverseSqrt(dot(bitangent, bitangent) + 1e-30);
    let mapped = t * xy.x + b * xy.y + normal * z;
    let length_squared = dot(mapped, mapped);
    return select(normal, mapped * inverseSqrt(max(length_squared, 1e-30)), length_squared > 1e-30);
}

fn shade_mesh(in: MeshOutput, two_sided: bool) -> vec4<f32> {
    let ambient = vec3<f32>(0.1);

    let normal = mesh_normal(in);
    let light_dir = normalize(camera.eye.xyz - in.world_position);

    var diffuse: f32;

    if two_sided {
        diffuse = abs(dot(normal, light_dir));
    } else {
        diffuse = max(dot(normal, light_dir), 0.0);
    }

    let light = ambient + vec3<f32>(diffuse);

    if camera.flags.x == 1u {
        return vec4<f32>(light * vec3<f32>(0.603), 1.0);
    }

    var color = textureSample(t_albedo, s_albedo, in.uv) * material.tint;
    if material.flags.z != 0u {
        color *= in.color;
    }
    if material.flags.y != 0u && color.a < bitcast<f32>(material.flags.w) {
        discard;
    }
    var albedo = color.rgb;

    if camera.flags.y == 1u {
        albedo = linear_to_srgb(albedo);
    }

    if material.flags.x != 0u {
        return vec4<f32>(albedo, color.a);
    }
    return vec4<f32>(light * albedo, color.a);
}

@fragment
fn fs_mesh(in: MeshOutput) -> @location(0) vec4<f32> {
    return shade_mesh(in, false);
}

@fragment
fn fs_mesh_two_sided(in: MeshOutput) -> @location(0) vec4<f32> {
    return shade_mesh(in, true);
}

// -- Lines (grid and bones) ---------------------------------------------------

struct LineInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
}

struct LineOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_position: vec3<f32>,
}

// Bone lines live in model space (up-axis correction applied).
@vertex
fn vs_line_model(in: LineInput) -> LineOutput {
    var out: LineOutput;

    let world_position = camera.model * vec4<f32>(in.position, 1.0);

    out.position = camera.view_proj * world_position;
    out.color = in.color;
    out.world_position = world_position.xyz;

    return out;
}

@fragment
fn fs_line(in: LineOutput) -> @location(0) vec4<f32> {
    return in.color;
}

// -- Infinite grid ------------------------------------------------------------
//
// A port of echo_engine's editor grid (shaders/grid.slang). A fullscreen
// triangle whose fragments cast an eye-relative ray against the y = 0 plane:
// the camera sits at the origin, so the hit point, the grid phase and the axis
// positions never carry the camera's world coordinates and stay precise far
// from the origin. Occlusion comes from the depth test instead of echo's depth
// texture read, because the viewport's depth buffer is multisampled.

const GRID_COLOR: vec3<f32> = vec3<f32>(0.18);
const GRID_X_AXIS_COLOR: vec3<f32> = vec3<f32>(0.55, 0.035, 0.045);
const GRID_Z_AXIS_COLOR: vec3<f32> = vec3<f32>(0.025, 0.19, 0.6);

struct GridOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

@vertex
fn vs_grid(@builtin(vertex_index) index: u32) -> GridOutput {
    // A single triangle covering the whole screen.
    let ndc = vec2<f32>(
        f32((index << 1u) & 2u) * 2.0 - 1.0,
        f32(index & 2u) * 2.0 - 1.0,
    );

    var out: GridOutput;

    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.ndc = ndc;

    return out;
}

// Coverage of the nearest grid line, anti-aliased with screen-space
// derivatives and faded out once lines crowd within a couple of pixels.
fn grid_line(coordinate: vec2<f32>) -> f32 {
    let derivative = max(fwidth(coordinate), vec2<f32>(0.00001));
    var line = 1.0 - saturate(abs(fract(coordinate - 0.5) - 0.5) / derivative);
    line *= 1.0 - smoothstep(0.2, 0.8, max(derivative.x, derivative.y));

    return max(line.x, line.y);
}

struct GridFragment {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

@fragment
fn fs_grid(in: GridOutput) -> GridFragment {
    // With the camera at the origin every point on the pixel's ray is
    // collinear with it, so the unprojected far point is the ray direction.
    let far = camera.grid_inverse * vec4<f32>(in.ndc, 1.0, 1.0);
    let ray = far.xyz / far.w;

    if abs(ray.y) < 0.000001 {
        discard;
    }

    let time = -camera.params.x / ray.y;

    if time <= 0.0 {
        discard;
    }

    // Eye-relative hit point on the plane.
    let p = ray * time;

    let clip = camera.grid_view * vec4<f32>(p, 1.0);
    let depth = clip.z / clip.w;

    if clip.w <= 0.0 || depth < 0.0 || depth > 1.0 {
        discard;
    }

    let fade = 1.0 - smoothstep(camera.params.w * 0.3, camera.params.w, length(p.xz));
    let minor = grid_line((p.xz + camera.grid_phase.xy) / camera.params.y) * 0.08;
    let major = grid_line((p.xz + camera.grid_phase.zw) / camera.params.z) * 0.14;

    let derivative = max(fwidth(p.xz), vec2<f32>(0.00001));
    let x = (1.0 - saturate(abs(p.z - camera.grid_axes.y) / derivative.y)) * camera.grid_axes.w;
    let z = (1.0 - saturate(abs(p.x - camera.grid_axes.x) / derivative.x)) * camera.grid_axes.z;

    var color = GRID_COLOR;
    var alpha = max(minor, major);

    if x > 0.01 {
        color = mix(color, GRID_X_AXIS_COLOR, x);
        alpha = max(alpha, x * 0.85);
    }

    if z > 0.01 {
        color = mix(color, GRID_Z_AXIS_COLOR, z);
        alpha = max(alpha, z * 0.85);
    }

    var out: GridFragment;

    out.color = vec4<f32>(color, alpha * fade);
    out.depth = depth;

    return out;
}

// -- Joint markers (solid spheres) --------------------------------------------

// Joint marker radius as a fraction of camera distance, giving a roughly
// constant on-screen size regardless of zoom or model scale.
const JOINT_SCREEN_SCALE: f32 = 0.006;

struct JointInput {
    // Shared unit sphere vertex (also the surface normal).
    @location(0) unit: vec3<f32>,
    // Per-instance bone position.
    @location(1) center: vec3<f32>,
}

struct JointOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
}

@vertex
fn vs_joint(in: JointInput) -> JointOutput {
    var out: JointOutput;

    let world_center = (camera.model * vec4<f32>(in.center, 1.0)).xyz;
    let world_dir = normalize((camera.model * vec4<f32>(in.unit, 0.0)).xyz);

    let radius = length(camera.eye.xyz - world_center) * JOINT_SCREEN_SCALE;
    let world_position = world_center + world_dir * radius;

    out.position = camera.view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    out.normal = world_dir;

    return out;
}

@fragment
fn fs_joint(in: JointOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.normal);
    let light_dir = normalize(camera.eye.xyz - in.world_position);
    let diffuse = max(dot(normal, light_dir), 0.0);
    let light = 0.35 + 0.65 * diffuse;

    return vec4<f32>(vec3<f32>(0.92, 0.92, 0.95) * light, 1.0);
}

// -- Images -------------------------------------------------------------------

struct ImageInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

struct ImageOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_image(in: ImageInput) -> ImageOutput {
    var out: ImageOutput;

    out.position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.uv = in.uv;

    return out;
}

@fragment
fn fs_image(in: ImageOutput) -> @location(0) vec4<f32> {
    var sample = textureSample(t_albedo, s_albedo, in.uv);

    if camera.flags.z == 1u {
        sample = vec4<f32>(sample.xxx, 1.0);
    }

    if camera.flags.y == 1u {
        return vec4<f32>(linear_to_srgb(sample.xyz), sample.w);
    }

    return sample;
}
