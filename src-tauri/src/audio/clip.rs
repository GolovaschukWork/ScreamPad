use std::sync::Arc;

/// Decoded audio ready to be mixed: interleaved `f32` frames in the output device's channel count and sample rate.
/// Immutable once created, and shared by reference counting.
#[derive(Debug, PartialEq)]
pub struct Clip {
    samples: Arc<[f32]>,
    channels: u16,
    sample_rate: u32,
}

impl Clip {
    /// `samples` must hold a whole number of frames; returns `None` otherwise or when `channels` or `sample_rate` is zero.
    pub fn new(samples: Vec<f32>, channels: u16, sample_rate: u32) -> Option<Arc<Self>> {
        let valid =
            channels > 0 && sample_rate > 0 && samples.len().is_multiple_of(usize::from(channels));
        valid.then(|| {
            Arc::new(Self {
                samples: samples.into(),
                channels,
                sample_rate,
            })
        })
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn duration_ms(&self) -> u64 {
        (self.frames() as u64 * 1000).div_ceil(u64::from(self.sample_rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clip_needs_whole_frames() {
        assert!(Clip::new(vec![0.0; 6], 2, 48_000).is_some());
        assert!(Clip::new(vec![0.0; 5], 2, 48_000).is_none());
        assert!(Clip::new(vec![0.0; 4], 0, 48_000).is_none());
        assert!(Clip::new(vec![0.0; 4], 2, 0).is_none());
    }

    #[test]
    fn frames_and_duration_follow_from_the_samples() {
        let clip = Clip::new(vec![0.0; 2 * 24_000], 2, 48_000).expect("clip");
        assert_eq!(clip.frames(), 24_000);
        assert_eq!(clip.duration_ms(), 500);
    }
}
