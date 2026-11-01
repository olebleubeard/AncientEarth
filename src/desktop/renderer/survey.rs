//! Canonical survey prisms. Tape annotations are projected by the GUI overlay.
use super::{
    materials::{EclipseSettings, GroundSettings},
    mesh_from,
};
use ancient_earth::survey::SurveyState;
use bevy::{
    asset::uuid_handle,
    ecs::system::SystemParam,
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};
use earth_math::{
    Vec3d,
    rendering::{
        LocalFrame,
        survey::{marker_mesh, marker_sign_frame, marker_sign_mesh},
    },
};

const SHADER: Handle<Shader> = uuid_handle!("ebcb6bcd-96bf-4b97-851e-4142a2fd4762");
#[derive(Clone, Copy, Debug, ShaderType)]
struct Settings {
    sky_fill: Vec4,
    eclipse: EclipseSettings,
}
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct Extension {
    #[uniform(100)]
    settings: Settings,
}
impl MaterialExtension for Extension {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}
type Material = ExtendedMaterial<StandardMaterial, Extension>;

struct Part {
    entity: Entity,
    mesh: Handle<Mesh>,
}
#[derive(Resource)]
pub struct Display {
    material: Handle<Material>,
    sign_material: Handle<StandardMaterial>,
    markers: Vec<Part>,
    signs: Vec<Part>,
    generation: Option<u64>,
    origin: Option<LocalFrame>,
    markers_visible: bool,
}
pub fn install(app: &mut App) {
    app.add_plugins(MaterialPlugin::<Material>::default())
        .add_systems(Startup, setup);
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            SHADER.id(),
            Shader::from_wesl(
                format!(
                    "{}\n{}",
                    include_str!("../shaders/survey.wesl"),
                    include_str!("../shaders/eclipse.wesl")
                ),
                "ancient_earth/survey.wesl",
            ),
        )
        .expect("original survey shader");
}
fn setup(
    mut commands: Commands,
    mut materials: ResMut<Assets<Material>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
) {
    let material = materials.add(Material {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.75,
            reflectance: 0.25,
            // Picking permits an eye inside a prism and returns its exit face.
            cull_mode: None,
            double_sided: true,
            ..default()
        },
        extension: Extension {
            settings: Settings {
                sky_fill: Vec4::ZERO,
                eclipse: EclipseSettings::default(),
            },
        },
    });
    let sign_material = standard_materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    commands.insert_resource(Display {
        material,
        sign_material,
        markers: Vec::new(),
        signs: Vec::new(),
        generation: None,
        origin: None,
        markers_visible: false,
    });
}
#[derive(SystemParam)]
pub struct Parameters<'w> {
    display: ResMut<'w, Display>,
    materials: ResMut<'w, Assets<Material>>,
}
impl Parameters<'_> {
    pub fn update(
        &mut self,
        state: &SurveyState,
        local: LocalFrame,
        eye: Vec3d,
        lighting: GroundSettings,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
    ) {
        let origin_changed = self.display.origin.is_none_or(|old| {
            old.origin != local.origin
                || old.basis.east != local.basis.east
                || old.basis.up != local.basis.up
        });
        let rebuild = self.display.generation != Some(state.generation) || origin_changed;
        if rebuild {
            while self.display.markers.len() > state.markers.len() {
                let part = self.display.markers.pop().unwrap();
                commands.entity(part.entity).despawn();
                meshes.remove(part.mesh.id());
            }
            while self.display.signs.len() > state.markers.len() {
                let part = self.display.signs.pop().unwrap();
                commands.entity(part.entity).despawn();
                meshes.remove(part.mesh.id());
            }
            for (index, marker) in state.markers.iter().enumerate() {
                let data = marker_mesh(marker, local);
                let mesh = mesh_from(data.mesh)
                    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, data.band_coordinates);
                if let Some(part) = self.display.markers.get(index) {
                    meshes
                        .insert(part.mesh.id(), mesh)
                        .expect("survey marker mesh retained");
                } else {
                    let mesh = meshes.add(mesh);
                    let entity = commands
                        .spawn((
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(self.display.material.clone()),
                            Visibility::Hidden,
                        ))
                        .id();
                    self.display.markers.push(Part { entity, mesh });
                }
                // A fixed local plate uses original geometry digits. Origin
                // changes and camera movement require transforms, not new text.
                if self.display.generation != Some(state.generation)
                    || index >= self.display.signs.len()
                {
                    let mesh = mesh_from(marker_sign_mesh(marker).unwrap_or_default());
                    if let Some(part) = self.display.signs.get(index) {
                        meshes
                            .insert(part.mesh.id(), mesh)
                            .expect("marker sign mesh retained");
                    } else {
                        let mesh = meshes.add(mesh);
                        let entity = commands
                            .spawn((
                                Mesh3d(mesh.clone()),
                                MeshMaterial3d(self.display.sign_material.clone()),
                                // These plates label prisms; they are not extra
                                // physical occluders for survey measurements.
                                bevy::light::NotShadowCaster,
                                bevy::light::NotShadowReceiver,
                                Visibility::Hidden,
                            ))
                            .id();
                        self.display.signs.push(Part { entity, mesh });
                    }
                }
            }
            self.display.generation = Some(state.generation);
            self.display.origin = Some(local);
        }
        if rebuild || state.markers_visible != self.display.markers_visible {
            for part in &self.display.markers {
                commands
                    .entity(part.entity)
                    .insert(if state.markers_visible {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    });
            }
            self.display.markers_visible = state.markers_visible;
        }
        for (part, marker) in self.display.signs.iter().zip(state.markers.iter()) {
            if state.markers_visible
                && let Some(frame) = marker_sign_frame(marker, eye)
            {
                let axes = Mat3::from_cols(
                    Vec3::from_array(local.direction(frame.right)),
                    Vec3::from_array(local.direction(frame.up)),
                    Vec3::from_array(local.direction(frame.normal)),
                );
                commands.entity(part.entity).insert((
                    Transform::from_translation(Vec3::from_array(local.position(frame.center)))
                        .with_rotation(Quat::from_mat3(&axes).normalize()),
                    Visibility::Visible,
                ));
            } else {
                commands.entity(part.entity).insert(Visibility::Hidden);
            }
        }
        if let Some(mut material) = self.materials.get_mut(&self.display.material) {
            material.extension.settings = Settings {
                sky_fill: lighting.sky_fill,
                eclipse: lighting.eclipse,
            };
        }
    }
}
