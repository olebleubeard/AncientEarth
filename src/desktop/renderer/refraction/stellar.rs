//! Same-frame requested catalogue arrivals. The physical pixel trace and final
//! stellar ownership remain authoritative; this only shares identical solves.
use super::Settings;
use bevy::{
    prelude::{UVec4, Vec4},
    render::render_resource::ShaderType,
};

pub(super) const REQUEST_BYTES: u64 = 3_000 * 4;
pub(super) const ARRIVAL_BYTES: u64 = 16 + 3_000 * 16;
pub(super) const STAR_GROUPS: u32 = 3_000_u32.div_ceil(32);

/// The zero stellar-weight branch performs no catalogue work in the original
/// compositor. Narrow lenses retain the same sparse request/ownership rules;
/// no arbitrary angular cutoff changes their physical rendering.
pub(super) fn eligible(s: &Settings) -> bool {
    s.control.x > 0 && s.control.y > 0 && s.environment.z > 0.0
}

#[derive(Clone, Debug, ShaderType)]
pub(super) struct StellarSettings {
    pub q0: Vec4,
    pub q1: Vec4,
    pub q2: Vec4,
    pub origin: Vec4,
    pub center: Vec4,
    pub budget: Vec4,
    pub scene: Vec4,
    pub lens: Vec4,
    pub icrf0: Vec4,
    pub icrf1: Vec4,
    pub icrf2: Vec4,
    pub environment: Vec4,
    pub control: UVec4,
    pub layers: [Vec4; 128],
}
impl StellarSettings {
    pub(super) fn from_view(s: &Settings, control: UVec4) -> Self {
        Self {
            q0: s.q0,
            q1: s.q1,
            q2: s.q2,
            origin: s.origin,
            center: s.center,
            budget: s.budget,
            scene: s.scene,
            lens: s.lens,
            icrf0: s.icrf0,
            icrf1: s.icrf1,
            icrf2: s.icrf2,
            environment: s.environment,
            control,
            layers: s.layers,
        }
    }
}

fn function<'a>(source: &'a str, name: &str) -> &'a str {
    let start = source
        .find(&format!("fn {name}("))
        .expect("stellar source function");
    let body = start + source[start..].find('{').expect("stellar function body");
    let mut depth = 0;
    for (offset, byte) in source.as_bytes()[body..].iter().enumerate() {
        if *byte == b'{' {
            depth += 1;
        }
        if *byte == b'}' {
            depth -= 1;
            if depth == 0 {
                return &source[start..body + offset + 1];
            }
        }
    }
    panic!("Unclosed stellar source function");
}

/// Reuse the production field and inverse algorithm verbatim. This compact
/// compute module contains no physical scene intersection or radiance work.
pub(super) fn stellar_shader() -> String {
    assert_eq!(earth_math::rendering::stars::STAR_COUNT, 3_000);
    assert_eq!(earth_math::rendering::stars::STAR_GRID, 32);
    let source = include_str!("../../shaders/refraction.wesl");
    let common = include_str!("../../shaders/stars_common.wesl");
    let core = &source[source.find("fn qmul(").expect("optical field")
        ..source.find("fn trace(").expect("physical trace")];
    format!(
        "{HEADER}\n{core}\n{}\n{}\n{}\n{}",
        function(source, "inverse_star"),
        function(source, "trace_atmosphere"),
        function(common, "star_face"),
        include_str!("../../shaders/refraction_stellar.wesl")
    )
}

const HEADER: &str = r#"
struct Settings {
    q0:vec4<f32>,q1:vec4<f32>,q2:vec4<f32>,origin:vec4<f32>,center:vec4<f32>,
    budget:vec4<f32>,scene:vec4<f32>,lens:vec4<f32>,
    icrf0:vec4<f32>,icrf1:vec4<f32>,icrf2:vec4<f32>,environment:vec4<f32>,
    control:vec4<u32>,layers:array<vec4<f32>,128>,
};
@group(0) @binding(0) var<uniform> settings:Settings;
@group(0) @binding(1) var<storage,read> rays:array<vec4<f32>>;
@group(0) @binding(2) var<storage,read_write> output:array<vec4<u32>>;
@group(0) @binding(3) var<storage,read> primary_terminals:array<vec4<u32>>;
@group(0) @binding(4) var<storage,read_write> stellar_requests:array<atomic<u32>>;
"#;

#[cfg(test)]
mod review;
