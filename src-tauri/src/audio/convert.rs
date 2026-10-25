//! Pure sample-layout conversions used when preparing a clip for the output device.

/// Converts interleaved samples from `from` channels to `to` channels.
///
/// - same count: unchanged;
/// - mono to N: the sample is copied to every channel;
/// - N to mono: the channels are averaged;
/// - otherwise the first `min(from, to)` channels are kept in place and any extra output channels are silent. Surround
///   sources therefore keep only their front pair on a stereo device; a proper downmix is a later improvement.
///
/// A trailing partial frame in `samples` is ignored. A zero channel count yields an empty result.
pub fn remap_channels(samples: Vec<f32>, from: usize, to: usize) -> Vec<f32> {
    if from == 0 || to == 0 {
        return Vec::new();
    }
    if from == to {
        return samples;
    }
    let frames = samples.len() / from;
    let mut out = Vec::with_capacity(frames * to);
    for frame in samples.chunks_exact(from) {
        if from == 1 {
            out.extend(std::iter::repeat_n(frame[0], to));
        } else if to == 1 {
            out.push(frame.iter().sum::<f32>() / from as f32);
        } else {
            out.extend((0..to).map(|channel| frame.get(channel).copied().unwrap_or(0.0)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_layout_is_returned_unchanged() {
        assert_eq!(
            remap_channels(vec![0.1, 0.2, 0.3, 0.4], 2, 2),
            vec![0.1, 0.2, 0.3, 0.4]
        );
    }

    #[test]
    fn mono_is_copied_to_every_output_channel() {
        assert_eq!(
            remap_channels(vec![0.5, -0.5], 1, 2),
            vec![0.5, 0.5, -0.5, -0.5]
        );
        assert_eq!(remap_channels(vec![0.5], 1, 3), vec![0.5, 0.5, 0.5]);
    }

    #[test]
    fn several_channels_are_averaged_into_mono() {
        assert_eq!(
            remap_channels(vec![0.2, 0.4, -0.2, 0.2], 2, 1),
            vec![0.3, 0.0]
        );
    }

    #[test]
    fn stereo_on_more_channels_leaves_the_rest_silent_and_surround_keeps_the_front_pair() {
        assert_eq!(
            remap_channels(vec![0.1, 0.2], 2, 4),
            vec![0.1, 0.2, 0.0, 0.0]
        );
        assert_eq!(
            remap_channels(vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6], 6, 2),
            vec![0.1, 0.2]
        );
    }

    #[test]
    fn degenerate_input_never_panics() {
        assert!(remap_channels(vec![0.1, 0.2], 0, 2).is_empty());
        assert!(remap_channels(vec![0.1, 0.2], 2, 0).is_empty());
        let averaged = remap_channels(vec![0.1, 0.2, 0.3], 2, 1);
        assert_eq!(averaged.len(), 1, "the trailing partial frame is ignored");
        assert!((averaged[0] - 0.15).abs() < 1e-6);
        assert!(remap_channels(Vec::new(), 2, 1).is_empty());
    }
}
