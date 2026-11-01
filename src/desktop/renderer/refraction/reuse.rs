//! Exact input identity for stationary primary rays. No interpolation or
//! numerical tolerance is used here; moving views still run the full solver.
use super::compute::PrimarySettings;
use bevy::render::{
    render_resource::{BufferId, BufferUsages, ComputePipelineId, encase},
    storage::ShaderBuffer,
};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Key {
    settings: Vec<u8>,
    buffers: [BufferId; 3],
    pipeline: ComputePipelineId,
}

impl Key {
    pub(super) fn matches_pipeline(&self, pipeline: ComputePipelineId) -> bool {
        self.pipeline == pipeline
    }

    pub(super) fn new(
        settings: &PrimarySettings,
        buffers: [BufferId; 3],
        pipeline: ComputePipelineId,
    ) -> Self {
        let mut settings = settings.clone();
        // These are the output generation token, never a physical input.
        // The viewport dimensions in x/y remain part of the exact key.
        settings.control.z = 0;
        settings.control.w = 0;
        let mut encoded = encase::UniformBuffer::new(Vec::new());
        encoded.write(&settings).expect("fixed primary uniform");
        Self {
            settings: encoded.into_inner(),
            buffers,
            pipeline,
        }
    }
}

pub(super) fn immutable(data: Vec<[f32; 4]>) -> ShaderBuffer {
    let mut buffer = ShaderBuffer::from(data);
    // Bevy may otherwise overwrite a buffer without changing its BufferId.
    // Read-only optical inputs must get a new identity when their data changes.
    buffer.buffer_usage.remove(BufferUsages::COPY_DST);
    buffer
}

#[derive(Default)]
pub(super) struct FrameUpload {
    words: Option<Vec<[u32; 4]>>,
}

impl FrameUpload {
    pub(super) fn changed(&mut self, data: Vec<[f32; 4]>) -> Option<ShaderBuffer> {
        if self.words.as_ref().is_some_and(|words| {
            words.len() == data.len()
                && words
                    .iter()
                    .zip(&data)
                    .all(|(old, new)| *old == new.map(f32::to_bits))
        }) {
            return None;
        }
        self.words = Some(data.iter().map(|value| value.map(f32::to_bits)).collect());
        Some(immutable(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::renderer::refraction::Settings;
    use bevy::prelude::UVec4;

    #[test]
    fn key_omits_only_token_and_preserves_every_other_serialized_bit() {
        let settings =
            PrimarySettings::from_view(&Settings::default(), UVec4::new(1920, 1080, 1, 2));
        let buffers = std::array::from_fn(|_| BufferId::new());
        let pipeline = ComputePipelineId::new();
        let key = Key::new(&settings, buffers, pipeline);
        let mut changed = settings.clone();
        changed.control.z = u32::MAX;
        changed.control.w = 99;
        assert!(key == Key::new(&changed, buffers, pipeline));
        // Change each dependency alone: changing several fields together
        // would let one correctly serialized field hide another omitted one.
        for field_index in 0..11 {
            for component in 0..4 {
                changed = settings.clone();
                let field = match field_index {
                    0 => &mut changed.q0,
                    1 => &mut changed.q1,
                    2 => &mut changed.q2,
                    3 => &mut changed.origin,
                    4 => &mut changed.center,
                    5 => &mut changed.right,
                    6 => &mut changed.up,
                    7 => &mut changed.forward,
                    8 => &mut changed.lens,
                    9 => &mut changed.budget,
                    10 => &mut changed.scene,
                    _ => unreachable!(),
                };
                field[component] = f32::from_bits(field[component].to_bits() ^ 1);
                assert!(key != Key::new(&changed, buffers, pipeline));
            }
        }
        changed = settings.clone();
        changed.control.x += 1;
        assert!(key != Key::new(&changed, buffers, pipeline));
        changed = settings.clone();
        changed.control.y += 1;
        assert!(key != Key::new(&changed, buffers, pipeline));
        for index in 0..settings.layers.len() {
            for component in 0..4 {
                changed = settings.clone();
                changed.layers[index][component] =
                    f32::from_bits(changed.layers[index][component].to_bits() ^ 1);
                assert!(key != Key::new(&changed, buffers, pipeline));
            }
        }
        // An independent encase byte oracle verifies that exactly the final
        // two words of the control vector are excluded, with no truncation.
        let mut encoded = encase::UniformBuffer::new(Vec::new());
        encoded.write(&settings).unwrap();
        let mut bytes = encoded.into_inner();
        assert_eq!(bytes.len(), 2240);
        bytes[11 * 16 + 8..12 * 16].fill(0);
        assert_eq!(key.settings, bytes);
        for index in 0..buffers.len() {
            let mut new_buffers = buffers;
            new_buffers[index] = BufferId::new();
            assert!(key != Key::new(&settings, new_buffers, pipeline));
        }
        assert!(key != Key::new(&settings, buffers, ComputePipelineId::new()));
    }

    #[test]
    fn frame_upload_uses_bits_including_signed_zero_and_nan_payloads() {
        let mut upload = FrameUpload::default();
        let original = vec![[0.0, f32::from_bits(0x7fc0_0001), 1.0, 2.0]];
        let first = upload.changed(original.clone()).unwrap();
        assert!(!first.buffer_usage.contains(BufferUsages::COPY_DST));
        assert!(upload.changed(original.clone()).is_none());
        let mut changed = original.clone();
        changed[0][0] = -0.0;
        assert!(upload.changed(changed.clone()).is_some());
        assert!(upload.changed(changed.clone()).is_none());
        changed[0][1] = f32::from_bits(0x7fc0_0002);
        assert!(upload.changed(changed).is_some());
        assert!(upload.changed(original.clone()).is_some());
        assert!(upload.changed(vec![]).is_some());
        assert!(upload.changed(vec![]).is_none());
        assert!(upload.changed(original).is_some());
    }
}
