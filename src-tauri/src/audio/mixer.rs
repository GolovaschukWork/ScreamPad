//! The mixer runs inside the audio callback.
//!
//! Rules it follows (ADR 0006): no allocation, no locks, no I/O, no logging, no blocking channel operations, no panics. The
//! control side talks to it only through bounded wait-free queues. A clip is never dropped here: when a voice ends, its
//! `Arc<Clip>` goes to the `retired` queue and the control thread frees it, so the last reference is never released in the
//! callback.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rtrb::{Consumer, Producer, PushError, RingBuffer};

use super::clip::Clip;

/// Voices that can sound at once. The array is allocated once, up front.
pub const MAX_VOICES: usize = 32;
const COMMAND_CAPACITY: usize = 256;
const EVENT_CAPACITY: usize = 256;
/// Enough for every voice to finish while the control thread is busy.
const RETIRED_CAPACITY: usize = MAX_VOICES * 4;
/// A stopped voice fades out over this long instead of being cut, which would click.
const STOP_FADE_MS: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VoiceId(pub u64);

/// Control to callback.
pub enum Command {
    Play {
        voice: VoiceId,
        clip: Arc<Clip>,
        gain: f32,
    },
    Stop(VoiceId),
    StopAll,
    SetMasterGain(f32),
}

/// Callback to control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The voice played to its end or was stopped.
    VoiceFinished(VoiceId),
    /// The voice was not started: no free slot, or the clip does not match the output channel count.
    PlayRejected(VoiceId),
}

/// Counters the callback updates and anyone may read.
#[derive(Debug, Default)]
pub struct Stats {
    pub callbacks: AtomicU64,
    /// Events that could not be queued because the control side was not draining them.
    pub dropped_events: AtomicU64,
    pub rejected_plays: AtomicU64,
    /// Clips that could not be handed back for freeing; they are leaked on purpose rather than freed in the callback.
    pub leaked_clips: AtomicU64,
}

/// A linear fade-out in progress.
struct Fade {
    remaining: u32,
    total: u32,
}

struct Voice {
    id: VoiceId,
    /// `None` once the clip has been handed to the `retired` queue.
    clip: Option<Arc<Clip>>,
    /// Next frame to play.
    position: usize,
    gain: f32,
    /// Set when the voice was stopped: it ramps down to silence before it ends.
    fade: Option<Fade>,
    /// Ended (finished or faded out) and waiting to be retired.
    ended: bool,
}

impl Voice {
    /// Starts the fade-out of a stopped voice. A voice that has not been heard yet is simply dropped, and a voice that is
    /// already fading or ended is left alone, so stopping twice never restarts or lengthens the fade.
    fn begin_fade(&mut self, frames: u32) {
        if self.ended || self.fade.is_some() {
            return;
        }
        if self.position == 0 {
            self.ended = true;
        } else {
            self.fade = Some(Fade {
                remaining: frames,
                total: frames,
            });
        }
    }
}

pub struct Mixer {
    channels: u16,
    /// Length of the fade-out of a stopped voice, in frames.
    fade_frames: u32,
    voices: [Option<Voice>; MAX_VOICES],
    master_gain: f32,
    commands: Consumer<Command>,
    events: Producer<Event>,
    retired: Producer<Arc<Clip>>,
    stats: Arc<Stats>,
}

/// The control side of a mixer.
pub struct MixerHandle {
    channels: u16,
    commands: Producer<Command>,
    events: Consumer<Event>,
    retired: Consumer<Arc<Clip>>,
    stats: Arc<Stats>,
    next_voice: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlError {
    /// The command queue is full; the callback is not keeping up or is not running.
    Busy,
    /// The clip's channel count differs from the mixer's.
    ChannelMismatch,
}

/// Creates a mixer for `channels` interleaved output channels and its control handle.
pub fn mixer(channels: u16, sample_rate: u32) -> (Mixer, MixerHandle) {
    let (command_tx, command_rx) = RingBuffer::new(COMMAND_CAPACITY);
    let (event_tx, event_rx) = RingBuffer::new(EVENT_CAPACITY);
    let (retired_tx, retired_rx) = RingBuffer::new(RETIRED_CAPACITY);
    let stats = Arc::new(Stats::default());
    let channels = channels.max(1);
    let fade_frames = u32::try_from(u64::from(sample_rate) * STOP_FADE_MS / 1000)
        .unwrap_or(u32::MAX)
        .max(1);
    let mixer = Mixer {
        channels,
        fade_frames,
        voices: std::array::from_fn(|_| None),
        master_gain: 1.0,
        commands: command_rx,
        events: event_tx,
        retired: retired_tx,
        stats: Arc::clone(&stats),
    };
    let handle = MixerHandle {
        channels,
        commands: command_tx,
        events: event_rx,
        retired: retired_rx,
        stats,
        next_voice: 0,
    };
    (mixer, handle)
}

impl Mixer {
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Fills `out` (interleaved, `channels` per frame) with the mix of all voices. Called from the audio callback.
    pub fn render(&mut self, out: &mut [f32]) {
        self.stats.callbacks.fetch_add(1, Ordering::Relaxed);
        self.apply_commands();
        out.fill(0.0);

        let channels = usize::from(self.channels);
        let out_frames = out.len() / channels;
        for slot in self.voices.iter_mut() {
            let Some(voice) = slot else { continue };
            if voice.ended {
                continue;
            }
            let Some(clip) = voice.clip.as_ref() else {
                continue;
            };
            let remaining = clip.frames().saturating_sub(voice.position);
            let frames = remaining.min(out_frames);
            let start = voice.position * channels;
            let source = clip
                .samples()
                .get(start..start + frames * channels)
                .unwrap_or(&[]);
            let gain = voice.gain;
            let mixed = match voice.fade.as_mut() {
                None => {
                    for (target, sample) in out.iter_mut().zip(source) {
                        *target += sample * gain;
                    }
                    frames
                }
                Some(fade) => {
                    // Ramp down frame by frame: full level at the first fade frame, silence after the last.
                    let ramp = frames.min(fade.remaining as usize);
                    let targets = out.chunks_exact_mut(channels);
                    let sources = source.chunks_exact(channels);
                    for (offset, (target, frame)) in targets.zip(sources).take(ramp).enumerate() {
                        let left = fade.remaining.saturating_sub(offset as u32);
                        let factor = gain * left as f32 / fade.total as f32;
                        for (target, sample) in target.iter_mut().zip(frame) {
                            *target += sample * factor;
                        }
                    }
                    fade.remaining -= ramp as u32;
                    if fade.remaining == 0 {
                        voice.ended = true;
                    }
                    ramp
                }
            };
            voice.position += mixed;
            if voice.position >= clip.frames() {
                voice.ended = true;
            }
        }

        let master = self.master_gain;
        for sample in out.iter_mut() {
            let value = *sample * master;
            // Overload and bad input must never reach the device as NaN or out-of-range values.
            *sample = if value.is_finite() {
                value.clamp(-1.0, 1.0)
            } else {
                0.0
            };
        }
        self.retire_ended();
    }

    fn apply_commands(&mut self) {
        while let Ok(command) = self.commands.pop() {
            match command {
                Command::Play { voice, clip, gain } => self.start(voice, clip, gain),
                Command::Stop(id) => {
                    let frames = self.fade_frames;
                    for voice in self.voices.iter_mut().flatten() {
                        if voice.id == id {
                            voice.begin_fade(frames);
                        }
                    }
                }
                Command::StopAll => {
                    let frames = self.fade_frames;
                    for voice in self.voices.iter_mut().flatten() {
                        voice.begin_fade(frames);
                    }
                }
                Command::SetMasterGain(gain) => {
                    if gain.is_finite() && gain >= 0.0 {
                        self.master_gain = gain;
                    }
                }
            }
        }
    }

    fn start(&mut self, id: VoiceId, clip: Arc<Clip>, gain: f32) {
        let gain = if gain.is_finite() && gain >= 0.0 {
            gain
        } else {
            1.0
        };
        let matches_output = clip.channels() == self.channels && clip.frames() > 0;
        let free = if matches_output {
            self.voices.iter_mut().find(|slot| slot.is_none())
        } else {
            None
        };
        match free {
            Some(slot) => {
                *slot = Some(Voice {
                    id,
                    clip: Some(clip),
                    position: 0,
                    gain,
                    fade: None,
                    ended: false,
                })
            }
            None => {
                self.stats.rejected_plays.fetch_add(1, Ordering::Relaxed);
                self.push_event(Event::PlayRejected(id));
                self.give_back(clip);
            }
        }
    }

    /// Hands ended voices' clips to the control side and frees their slots. A voice whose clip cannot be handed over yet
    /// keeps its slot and is retried on the next callback.
    fn retire_ended(&mut self) {
        for index in 0..MAX_VOICES {
            let Some(voice) = self.voices[index].as_mut() else {
                continue;
            };
            if !voice.ended {
                continue;
            }
            if let Some(clip) = voice.clip.take()
                && let Err(PushError::Full(clip)) = self.retired.push(clip)
            {
                voice.clip = Some(clip);
                continue;
            }
            let id = voice.id;
            self.voices[index] = None;
            self.push_event(Event::VoiceFinished(id));
        }
    }

    fn give_back(&mut self, clip: Arc<Clip>) {
        if let Err(PushError::Full(clip)) = self.retired.push(clip) {
            // Dropping it here could free memory in the callback. Leak it and count it; this only happens when the
            // control side has stopped draining the queue, which is a bug elsewhere.
            self.stats.leaked_clips.fetch_add(1, Ordering::Relaxed);
            std::mem::forget(clip);
        }
    }

    fn push_event(&mut self, event: Event) {
        if self.events.push(event).is_err() {
            self.stats.dropped_events.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl MixerHandle {
    pub fn stats(&self) -> &Arc<Stats> {
        &self.stats
    }

    /// Starts `clip` on a free voice at `gain` (1.0 is unity). The clip must have the mixer's channel count.
    pub fn play(&mut self, clip: Arc<Clip>, gain: f32) -> Result<VoiceId, ControlError> {
        if clip.channels() != self.channels {
            return Err(ControlError::ChannelMismatch);
        }
        let voice = VoiceId(self.next_voice);
        match self.commands.push(Command::Play { voice, clip, gain }) {
            Ok(()) => {
                self.next_voice += 1;
                Ok(voice)
            }
            Err(_) => Err(ControlError::Busy),
        }
    }

    pub fn stop(&mut self, voice: VoiceId) -> Result<(), ControlError> {
        self.commands
            .push(Command::Stop(voice))
            .map_err(|_| ControlError::Busy)
    }

    pub fn stop_all(&mut self) -> Result<(), ControlError> {
        self.commands
            .push(Command::StopAll)
            .map_err(|_| ControlError::Busy)
    }

    pub fn set_master_gain(&mut self, gain: f32) -> Result<(), ControlError> {
        self.commands
            .push(Command::SetMasterGain(gain))
            .map_err(|_| ControlError::Busy)
    }

    /// Frees clips that finished playing and returns the events since the last call. Call this regularly from the control
    /// thread; the callback relies on it to hand memory back.
    pub fn drain(&mut self) -> Vec<Event> {
        while self.retired.pop().is_ok() {}
        let mut events = Vec::new();
        while let Ok(event) = self.events.pop() {
            events.push(event);
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(value: f32, frames: usize, channels: u16) -> Arc<Clip> {
        Clip::new(
            vec![value; frames * usize::from(channels)],
            channels,
            48_000,
        )
        .expect("clip")
    }

    fn render(mixer: &mut Mixer, frames: usize) -> Vec<f32> {
        let mut out = vec![9.0; frames * usize::from(mixer.channels())];
        mixer.render(&mut out);
        out
    }

    #[test]
    fn without_voices_the_output_is_silence() {
        let (mut mixer, _handle) = mixer(2, 48_000);
        assert_eq!(render(&mut mixer, 4), vec![0.0; 8]);
    }

    #[test]
    fn a_clip_plays_once_across_buffers_and_then_finishes() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        let voice = handle.play(clip(0.5, 10, 1), 1.0).expect("play");

        assert_eq!(render(&mut mixer, 4), vec![0.5; 4]);
        assert_eq!(render(&mut mixer, 4), vec![0.5; 4]);
        assert_eq!(render(&mut mixer, 4), vec![0.5, 0.5, 0.0, 0.0]);
        assert_eq!(handle.drain(), vec![Event::VoiceFinished(voice)]);
        assert_eq!(render(&mut mixer, 4), vec![0.0; 4], "nothing keeps playing");
        assert!(handle.drain().is_empty());
    }

    #[test]
    fn the_buffers_of_a_long_clip_reassemble_into_the_original_samples() {
        let (mut mixer, mut handle) = mixer(2, 48_000);
        let samples: Vec<f32> = (0..2 * 1000).map(|i| (i as f32 / 4000.0) - 0.25).collect();
        handle
            .play(Clip::new(samples.clone(), 2, 48_000).expect("clip"), 1.0)
            .expect("play");
        let mut played = Vec::new();
        for size in [1, 7, 64, 129, 799] {
            played.extend(render(&mut mixer, size));
        }
        assert_eq!(&played[..samples.len()], samples.as_slice());
        assert!(played[samples.len()..].iter().all(|s| *s == 0.0));
    }

    #[test]
    fn stereo_channels_stay_in_place() {
        let (mut mixer, mut handle) = mixer(2, 48_000);
        handle
            .play(
                Clip::new(vec![0.1, 0.2, 0.1, 0.2], 2, 48_000).expect("clip"),
                1.0,
            )
            .expect("play");
        assert_eq!(render(&mut mixer, 2), vec![0.1, 0.2, 0.1, 0.2]);
    }

    #[test]
    fn voices_sum_and_gains_apply() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        handle.play(clip(0.25, 8, 1), 1.0).expect("play");
        handle.play(clip(0.25, 8, 1), 2.0).expect("play");
        assert_eq!(render(&mut mixer, 2), vec![0.75; 2]);
        handle.set_master_gain(0.5).expect("gain");
        handle.play(clip(0.25, 8, 1), 1.0).expect("play");
        assert_eq!(
            render(&mut mixer, 2),
            vec![0.5; 2],
            "(0.25 + 0.5 + 0.25) * 0.5"
        );
    }

    #[test]
    fn overload_is_clamped_and_never_exceeds_full_scale() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        handle.play(clip(0.8, 4, 1), 1.0).expect("play");
        handle.play(clip(0.8, 4, 1), 1.0).expect("play");
        assert_eq!(
            render(&mut mixer, 4),
            vec![1.0; 4],
            "1.6 is clamped to full scale"
        );

        handle.play(clip(-0.9, 4, 1), 1.0).expect("play");
        handle.play(clip(-0.9, 4, 1), 1.0).expect("play");
        assert_eq!(
            render(&mut mixer, 4),
            vec![-1.0; 4],
            "-1.8 is clamped to negative full scale"
        );

        // Voices are summed first and clamped once, so opposite signs cancel instead of each being clipped.
        handle.play(clip(0.8, 4, 1), 1.0).expect("play");
        handle.play(clip(-0.9, 4, 1), 1.0).expect("play");
        let out = render(&mut mixer, 4);
        assert!(out.iter().all(|s| (s + 0.1).abs() < 1e-6), "{out:?}");
    }

    #[test]
    fn bad_samples_and_gains_never_reach_the_output() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        handle
            .play(
                Clip::new(
                    vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.25],
                    1,
                    48_000,
                )
                .expect("clip"),
                1.0,
            )
            .expect("play");
        assert_eq!(render(&mut mixer, 4), vec![0.0, 0.0, 0.0, 0.25]);

        handle.play(clip(0.5, 2, 1), f32::NAN).expect("play");
        assert_eq!(
            render(&mut mixer, 2),
            vec![0.5; 2],
            "an invalid voice gain falls back to unity"
        );
        handle.set_master_gain(f32::NAN).expect("gain");
        handle.set_master_gain(-1.0).expect("gain");
        handle.play(clip(0.5, 2, 1), 1.0).expect("play");
        assert_eq!(
            render(&mut mixer, 2),
            vec![0.5; 2],
            "invalid master gains are ignored"
        );
    }

    /// Frames of the fade-out at 48 kHz: 5 ms.
    const FADE: usize = 240;

    #[test]
    fn stop_and_stop_all_fade_voices_out_and_report_them() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        let first = handle.play(clip(0.1, 5000, 1), 1.0).expect("play");
        let second = handle.play(clip(0.1, 5000, 1), 1.0).expect("play");
        let third = handle.play(clip(0.1, 5000, 1), 1.0).expect("play");
        render(&mut mixer, 10);

        handle.stop(first).expect("stop");
        let out = render(&mut mixer, 2 * FADE);
        assert!(
            out[0] > 0.29,
            "the stopped voice is still heard at the start of its fade: {}",
            out[0]
        );
        assert!(
            out[FADE..].iter().all(|s| (s - 0.2).abs() < 1e-6),
            "after the fade only the two other voices sound"
        );

        handle.stop_all().expect("stop all");
        let out = render(&mut mixer, 2 * FADE);
        assert!(
            out[FADE..].iter().all(|s| *s == 0.0),
            "silent once every fade is over"
        );

        let mut finished: Vec<VoiceId> = handle
            .drain()
            .into_iter()
            .map(|event| match event {
                Event::VoiceFinished(id) => id,
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        finished.sort_by_key(|id| id.0);
        assert_eq!(finished, vec![first, second, third]);
    }

    #[test]
    fn a_stopped_voice_ramps_down_smoothly_instead_of_cutting_off() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        let voice = handle.play(clip(0.5, 10_000, 1), 1.0).expect("play");
        render(&mut mixer, 100);

        handle.stop(voice).expect("stop");
        let out = render(&mut mixer, 2 * FADE);

        assert!(
            (out[0] - 0.5).abs() < 1e-3,
            "the fade starts at the current level: {}",
            out[0]
        );
        assert!(
            out[..FADE].windows(2).all(|pair| pair[1] <= pair[0]),
            "the level only goes down"
        );
        assert!(
            out[FADE - 1] < 0.5 / FADE as f32 * 1.5,
            "the last fade frame is nearly silent: {}",
            out[FADE - 1]
        );
        assert!(out[FADE..].iter().all(|s| *s == 0.0));
        // No step between neighbouring samples is larger than one step of the ramp, so there is no click.
        let step = 0.5 / FADE as f32;
        assert!(
            out.windows(2)
                .all(|pair| (pair[1] - pair[0]).abs() <= step * 1.01),
            "a discontinuity in the fade"
        );
        assert_eq!(handle.drain(), vec![Event::VoiceFinished(voice)]);
    }

    #[test]
    fn the_fade_continues_across_buffer_boundaries() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        let voice = handle.play(clip(0.5, 10_000, 1), 1.0).expect("play");
        render(&mut mixer, 100);
        handle.stop(voice).expect("stop");

        let mut all = Vec::new();
        for size in [1, 7, 64, 100, 200] {
            all.extend(render(&mut mixer, size));
        }
        let step = 0.5 / FADE as f32;
        assert!(
            all.windows(2)
                .all(|pair| (pair[1] - pair[0]).abs() <= step * 1.01)
        );
        assert!(all[FADE..].iter().all(|s| *s == 0.0));
    }

    #[test]
    fn a_voice_stopped_before_it_was_ever_heard_is_dropped_silently() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        let voice = handle.play(clip(0.5, 1000, 1), 1.0).expect("play");
        handle.stop(voice).expect("stop");
        assert_eq!(
            render(&mut mixer, 300),
            vec![0.0; 300],
            "there is nothing to fade"
        );
        assert_eq!(handle.drain(), vec![Event::VoiceFinished(voice)]);
    }

    #[test]
    fn stopping_twice_does_not_restart_or_lengthen_the_fade() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        let voice = handle.play(clip(0.5, 10_000, 1), 1.0).expect("play");
        render(&mut mixer, 100);
        handle.stop(voice).expect("stop");
        render(&mut mixer, 100);
        handle.stop(voice).expect("stop again");
        handle.stop_all().expect("and stop all");

        let out = render(&mut mixer, 300);
        assert!(
            out[140..].iter().all(|s| *s == 0.0),
            "the fade still ended 240 frames after the first stop"
        );
    }

    #[test]
    fn the_fade_is_five_milliseconds_at_any_sample_rate() {
        let (mut mixer, mut handle) = mixer(1, 1000);
        let voice = handle
            .play(Clip::new(vec![0.5; 100], 1, 1000).expect("clip"), 1.0)
            .expect("play");
        render(&mut mixer, 3);
        handle.stop(voice).expect("stop");
        let out = render(&mut mixer, 12);
        assert!(
            out[0] > 0.0 && out[4] > 0.0,
            "5 frames of fade at 1 kHz: {out:?}"
        );
        assert!(out[5..].iter().all(|s| *s == 0.0), "{out:?}");
    }

    #[test]
    fn a_clip_that_ends_during_its_fade_finishes_once() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        let voice = handle.play(clip(0.5, 150, 1), 1.0).expect("play");
        render(&mut mixer, 100);
        handle.stop(voice).expect("stop");
        let out = render(&mut mixer, 400);
        assert!(out[50..].iter().all(|s| *s == 0.0));
        assert_eq!(
            handle.drain(),
            vec![Event::VoiceFinished(voice)],
            "one event, not two"
        );
    }

    #[test]
    fn a_new_sound_can_start_while_another_fades() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        let old = handle.play(clip(0.25, 10_000, 1), 1.0).expect("play");
        render(&mut mixer, 50);
        handle.stop(old).expect("stop");
        handle
            .play(clip(0.25, 10_000, 1), 1.0)
            .expect("play the replacement");
        let out = render(&mut mixer, 2 * FADE);
        assert!(
            out[0] > 0.45,
            "the old voice at full level plus the new one: {}",
            out[0]
        );
        assert!(
            out[FADE..].iter().all(|s| (s - 0.25).abs() < 1e-6),
            "only the new voice remains"
        );
    }

    #[test]
    fn stopping_an_unknown_voice_is_harmless() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        handle.stop(VoiceId(12345)).expect("stop");
        assert_eq!(render(&mut mixer, 2), vec![0.0; 2]);
        assert!(handle.drain().is_empty());
    }

    #[test]
    fn voices_beyond_the_limit_are_rejected_not_allocated() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        for _ in 0..MAX_VOICES {
            handle.play(clip(0.001, 100, 1), 1.0).expect("play");
        }
        let extra = handle.play(clip(0.001, 100, 1), 1.0).expect("queued");
        render(&mut mixer, 1);
        assert_eq!(handle.drain(), vec![Event::PlayRejected(extra)]);
        assert_eq!(handle.stats().rejected_plays.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_clip_with_the_wrong_channel_count_is_refused_on_both_sides() {
        let (mut mixer, mut handle) = mixer(2, 48_000);
        assert_eq!(
            handle.play(clip(0.5, 4, 1), 1.0),
            Err(ControlError::ChannelMismatch)
        );

        // Even if one slipped through, the callback rejects it instead of misplaying it.
        let bad = clip(0.5, 4, 1);
        handle
            .commands
            .push(Command::Play {
                voice: VoiceId(99),
                clip: bad,
                gain: 1.0,
            })
            .expect("push");
        assert_eq!(render(&mut mixer, 2), vec![0.0; 4]);
        assert_eq!(handle.drain(), vec![Event::PlayRejected(VoiceId(99))]);
    }

    #[test]
    fn an_empty_clip_is_rejected() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        handle
            .commands
            .push(Command::Play {
                voice: VoiceId(1),
                clip: clip(0.5, 0, 1),
                gain: 1.0,
            })
            .expect("push");
        render(&mut mixer, 1);
        assert_eq!(handle.drain(), vec![Event::PlayRejected(VoiceId(1))]);
    }

    #[test]
    fn a_full_command_queue_reports_busy_instead_of_growing() {
        let (_mixer, mut handle) = mixer(1, 48_000);
        for _ in 0..COMMAND_CAPACITY {
            handle.stop_all().expect("fits");
        }
        assert_eq!(handle.stop_all(), Err(ControlError::Busy));
        assert_eq!(handle.play(clip(0.1, 1, 1), 1.0), Err(ControlError::Busy));
    }

    #[test]
    fn the_last_reference_to_a_clip_is_released_by_the_control_side_not_the_callback() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        let shared = clip(0.5, 2, 1);
        handle.play(Arc::clone(&shared), 1.0).expect("play");
        assert_eq!(Arc::strong_count(&shared), 2, "ours and the queued command");

        render(&mut mixer, 4);
        // The voice ended inside the callback, yet the callback released nothing: the queue holds the reference.
        assert_eq!(Arc::strong_count(&shared), 2);
        handle.drain();
        assert_eq!(Arc::strong_count(&shared), 1, "freed by the control side");
    }

    #[test]
    fn a_stalled_control_thread_never_makes_the_callback_free_memory() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        // Fill the retired queue completely without draining it.
        for _ in 0..RETIRED_CAPACITY / MAX_VOICES {
            for _ in 0..MAX_VOICES {
                handle.play(clip(0.001, 1, 1), 1.0).expect("play");
            }
            render(&mut mixer, 1);
        }
        assert_eq!(handle.stats().leaked_clips.load(Ordering::Relaxed), 0);

        // Voices that end now cannot be retired, so they keep their slots instead of dropping their clips.
        let held: Vec<Arc<Clip>> = (0..MAX_VOICES).map(|_| clip(0.001, 1, 1)).collect();
        for held_clip in &held {
            handle.play(Arc::clone(held_clip), 1.0).expect("play");
        }
        render(&mut mixer, 1);
        assert!(
            held.iter().all(|c| Arc::strong_count(c) == 2),
            "still referenced from the voice slots"
        );

        // A rejected clip has nowhere to go either: it is leaked and counted, not freed here.
        let rejected = clip(0.001, 1, 1);
        handle.play(Arc::clone(&rejected), 1.0).expect("queued");
        render(&mut mixer, 1);
        assert_eq!(handle.stats().leaked_clips.load(Ordering::Relaxed), 1);
        assert_eq!(
            Arc::strong_count(&rejected),
            2,
            "the leaked reference is never released"
        );

        // Once the control side drains, everything recovers.
        handle.drain();
        render(&mut mixer, 1);
        let events = handle.drain();
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, Event::VoiceFinished(_)))
                .count(),
            MAX_VOICES
        );
        assert!(held.iter().all(|c| Arc::strong_count(c) == 1));
    }

    #[test]
    fn a_control_thread_that_never_reads_events_cannot_block_the_callback() {
        let (mut mixer, mut handle) = mixer(1, 48_000);
        for _ in 0..(EVENT_CAPACITY / MAX_VOICES + 3) {
            for _ in 0..MAX_VOICES {
                handle.play(clip(0.001, 1, 1), 1.0).expect("play");
            }
            render(&mut mixer, 1);
            // Drain only the retired clips, never the events.
            while handle.retired.pop().is_ok() {}
        }
        assert!(handle.stats().dropped_events.load(Ordering::Relaxed) > 0);
        assert_eq!(
            render(&mut mixer, 2),
            vec![0.0; 2],
            "the mixer keeps working"
        );
    }

    #[test]
    fn an_odd_sized_buffer_ignores_the_trailing_partial_frame() {
        let (mut mixer, mut handle) = mixer(2, 48_000);
        handle.play(clip(0.5, 4, 2), 1.0).expect("play");
        let mut out = vec![9.0; 5];
        mixer.render(&mut out);
        assert_eq!(out, vec![0.5, 0.5, 0.5, 0.5, 0.0]);
    }
}
