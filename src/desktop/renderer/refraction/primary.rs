//! Lossless native-pixel primary-trace contract. Registration is kept separate
//! while the compute path is validated against the existing fragment path.
use super::{Settings, compute::PrimarySettings};
use bevy::prelude::UVec4;

impl PrimarySettings {
    pub(super) fn from_view(s: &Settings, control: UVec4) -> Self {
        Self {
            q0: s.q0,
            q1: s.q1,
            q2: s.q2,
            origin: s.origin,
            center: s.center,
            right: s.right,
            up: s.up,
            forward: s.forward,
            lens: s.lens,
            budget: s.budget,
            scene: s.scene,
            control,
            layers: s.layers,
        }
    }
}

/// The exact production primary-trace functions, with a compact binding set.
/// Pixel-centre construction is the same rectilinear/equidistant model. Actual
/// raster interpolation and compute arithmetic still require native validation.
pub(super) fn primary_shader() -> String {
    let source = include_str!("../../shaders/refraction.wesl");
    let core = &source[source.find("fn qmul").expect("optical field")
        ..source.find("fn relative_angles").expect("optical shading")];
    let scene = include_str!("../../shaders/refraction_scene.wesl");
    let scene = scene[..scene.find("fn sign_row(").expect("optical signs")]
        .replace("constants::MATERIAL_BIND_GROUP", "0")
        .replace("@binding(9)", "@binding(1)")
        .replace("@binding(10)", "@binding(2)");
    format!("{}\n{core}\n{scene}\n{}", HEADER, ENTRY)
}

const HEADER: &str = r#"
struct Settings {
    q0:vec4<f32>,q1:vec4<f32>,q2:vec4<f32>,origin:vec4<f32>,center:vec4<f32>,
    right:vec4<f32>,up:vec4<f32>,forward:vec4<f32>,lens:vec4<f32>,
    budget:vec4<f32>,scene:vec4<f32>,control:vec4<u32>,layers:array<vec4<f32>,128>,
};
@group(0) @binding(0) var<uniform> settings:Settings;
@group(0) @binding(3) var<storage,read_write> terminals:array<vec4<u32>>;
"#;

const ENTRY: &str = r#"
@compute @workgroup_size(32,1,1)
fn primary_trace(@builtin(global_invocation_id) id:vec3<u32>){
    let dimensions=settings.control.xy;
    if id.x>=dimensions.x || id.y>=dimensions.y{return;}
    if id.x==0u && id.y==0u{terminals[0]=settings.control;}
    let index=1u+3u*(id.y*dimensions.x+id.x);
    let uv=(vec2<f32>(id.xy)+vec2(0.5))/vec2<f32>(dimensions);
    let xy=vec2((2.0*uv.x-1.0)*settings.lens.x,1.0-2.0*uv.y);
    var optical=normalize(vec3(xy*tan(settings.lens.y),1.0));
    if settings.lens.z>0.5{
        let radius=length(xy);
        if radius>1.0{
            terminals[index]=vec4(0u);terminals[index+1u]=vec4(0u,0u,0u,3u);terminals[index+2u]=vec4(0u);return;
        }
        let theta=radius*settings.lens.y;
        let scale=select(settings.lens.y,sin(theta)/max(radius,1e-8),radius>1e-8);
        optical=vec3(xy*scale,cos(theta));
    }
    let direction=normalize(settings.right.xyz*optical.x+settings.up.xyz*optical.y+settings.forward.xyz*optical.z);
    let ray=trace(direction);
    terminals[index]=bitcast<vec4<u32>>(vec4(ray.p,ray.path));
    terminals[index+1u]=vec4(bitcast<vec3<u32>>(ray.u),ray.status);
    // Material is 0..2; canonical packet limits keep every owner below2^24.
    terminals[index+2u]=vec4(bitcast<vec3<u32>>(ray.normal),(ray.material<<24u)|ray.marker);
}
"#;
