//! Embedded, licensed NASA lunar presentation maps and shared orientation packet.
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, ShaderType, TextureDimension, TextureFormat},
};
use earth_math::{Vec3d, astro::AstronomyFrame, rendering::LocalFrame};
use std::io::Read;

#[cfg(test)]
mod review;

#[derive(Resource)]
pub struct LunarMaps {
    pub color: Handle<Image>,
    pub normal: Handle<Image>,
}

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct LunarSettings {
    pub body_x: Vec4,
    pub body_y: Vec4,
    pub body_z: Vec4,
    pub illumination: Vec4,
}
impl LunarSettings {
    pub fn at(local: LocalFrame, frame: &AstronomyFrame) -> Self {
        let axis =
            |v| Vec3::from_array(local.direction(frame.moon.body_to_ecef.transform(v))).extend(0.0);
        Self {
            body_x: axis(Vec3d::new(1.0, 0.0, 0.0)),
            body_y: axis(Vec3d::new(0.0, 1.0, 0.0)),
            body_z: axis(Vec3d::new(0.0, 0.0, 1.0)),
            illumination: Vec3::from_array(local.direction(frame.moon_to_sun_direction_ecef))
                .extend(0.0),
        }
    }
}

fn image(bytes: &[u8]) -> Image {
    let mut data = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(256 * 1024 * 1024)
        .read_to_end(&mut data)
        .expect("embedded lunar texture decompression");
    assert_eq!(&data[..8], b"AELUNAR1");
    let word = |i| u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
    let (width, height, levels, srgb) = (word(8), word(12), word(16), word(20));
    assert!(width > 0 && width <= 8192 && height > 0 && height <= 4096 && srgb <= 1);
    assert_eq!(levels, 32 - width.max(height).leading_zeros());
    let expected: usize = (0..levels)
        .map(|l| ((width >> l).max(1) * (height >> l).max(1) * 4) as usize)
        .sum();
    assert_eq!(data.len(), 24 + expected);
    let mut image = Image::new_uninit(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        if srgb == 1 {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = levels;
    image.data = Some(data.split_off(24));
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..ImageSamplerDescriptor::linear()
    });
    image
}
pub fn install(app: &mut App) {
    let mut images = app.world_mut().resource_mut::<Assets<Image>>();
    let maps = LunarMaps {
        color: images.add(image(include_bytes!("../../../assets/lunar/color.ael"))),
        normal: images.add(image(include_bytes!("../../../assets/lunar/normal.ael"))),
    };
    app.insert_resource(maps);
}
