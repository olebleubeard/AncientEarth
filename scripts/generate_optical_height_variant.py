"""Original MIT. Reproduce the diagnostic scalar-height DP54 transcription.

The source tableau is kept verbatim from the reviewed carried prototype;
only the redundant height coordinate is integrated, while every stage normal
comes from the existing inverse WGS84 geometry function.
"""
from pathlib import Path
import re

root = Path(__file__).resolve().parents[1]
source = (root / "src/desktop/shaders/refraction_dopri_carried.wesl").read_text()
source = source.replace("carried auxiliary Hermite", "scalar-height Hermite")
source = source.replace("gr2:vec4<f32>,gr3:vec4<f32>,gr4:vec4<f32>,gr5:vec4<f32>", "hr:vec4<f32>")
source = source.replace(",vec4(0.0),vec4(0.0),vec4(0.0),vec4(0.0));}", ",vec4(0.0));}")
begin = source.index("struct AuxField")
end = source.index("// branch=", begin)
source = source[:begin] + source[end:]
source = source.replace("let auxiliary0=aux_field(g0,s.u);if auxiliary0.valid==0u{return dp_invalid();}", "let h0=dot(s.u,g0.xyz);")
source = source.replace("auxiliary0.g.xyz", "g0.xyz")
for i in range(1, 7):
    old = (f"    let raw{i}=g0+increment{i};\n"
           f"    let auxiliary{i}=aux_field(raw{i},u{i});if auxiliary{i}.valid==0u{{return dp_invalid();}}\n"
           f"    let g{i}=auxiliary{i}.g;")
    assert old in source
    source = source.replace(old, f"    let g{i}=geometry(s.p+delta{i});let h{i}=dot(u{i},g{i}.xyz);")
    source = source.replace(f"increment{i}.w", f"increment{i}")
for i in range(7):
    source = source.replace(f"auxiliary{i}.derivative.w", f"h{i}")
    source = source.replace(f"auxiliary{i}.derivative", f"h{i}")
source = source.replace("+vec4(0.0)", "+0.0")
begin = source.index("    let auxiliary_error=")
end = source.index("    let ur3=", begin)
source = source[:begin] + """    let height_error=((h2-h0)*(-71.0/16695.0)+(h3-h0)*(71.0/1920.0)+(h4-h0)*(-17253.0/339200.0)+(h5-h0)*(22.0/525.0)+(h6-h0)*(-1.0/40.0))*step;
    if !dp_finite(vec4(ep,en)) || !dp_finite(vec4(eu,height_error)){return dp_invalid();}
    let embedded=max(max(max(length(ep),length(eu)),abs(en)),abs(height_error));
""" + source[end:]
begin = source.index("    let m=dp_normal_lower")
end = source.index("    let error=", begin)
source = source[:begin] + """    let m=dp_normal_lower(s.u,v6,ur3,ur4,ur5);
    if !(m>0.0) || !dp_finite(vec4(gr2,gr3,gr4,gr5)){return dp_invalid();}
    let interior=max(max(max(length(r5),2.0*step*length(ur5)/m),abs(nr5)),abs(gr5))/16.0;
""" + source[end:]
source = source.replace("ur5,gr2,gr3,gr4,gr5);", "ur5,vec4(gr2,gr3,gr4,gr5));")
(root / "src/desktop/shaders/refraction_dopri_height.wesl").write_text(source)

source = (root / "src/desktop/shaders/refraction_dopri_carried_trace.wesl").read_text()
source = source.replace("carried auxiliary direct dense", "scalar-height direct dense")
for i, component in enumerate("xyzw", 2):
    source = source.replace(f"dp_curve.gr{i}.w", f"dp_curve.hr.{component}")
source = source.replace("fn dp_auxiliary_delta(distance:f32)->vec4<f32>{", "fn dp_auxiliary_delta(distance:f32)->f32{")
for i, component in enumerate("xyzw", 2):
    source = source.replace(f"dp_curve.gr{i}", f"dp_curve.hr.{component}")
source = re.sub(r"dp_auxiliary_delta\(([^)]+)\)\.w", r"dp_auxiliary_delta(\1)", source)
source = source.replace("let change=dp_auxiliary_delta(distance);let raw=dp_curve_start+change;\n    result.geometry=vec4(normalize(raw.xyz),dp_branch().x+(dp_curve_offset+change.w));return result;",
                        "let change=dp_auxiliary_delta(distance);\n    result.geometry.w=dp_branch().x+(dp_curve_offset+change);return result;")
begin = source.index("fn dp_consistency(")
end = source.index("fn dp_advance(", begin)
source = source[:begin] + """// Only H is redundant state here. The physical normal is always computed
// from position, and is refreshed before any accepted-state commit.
fn dp_consistency(aux:vec4<f32>,actual:vec4<f32>)->f32{
    if !dp_finite(aux)||!dp_finite(actual){return 1e30;}
    return abs(aux.w-actual.w);
}
""" + source[end:]
source = source.replace("var end_actual=geometry(state.p+accepted.delta.p);", "var end_actual=geometry(state.p+accepted.delta.p);accepted.geometry=vec4(end_actual.xyz,accepted.geometry.w);")
(root / "src/desktop/shaders/refraction_dopri_height_trace.wesl").write_text(source)
