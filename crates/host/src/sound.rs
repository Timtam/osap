//! `host.sound`'s audio output — and opening it again when the device it was opened on is gone.
//!
//! **Why not `rodio::OutputStream`.** It opens the default device once, and cpal binds the stream
//! to that endpoint for as long as it lives. After the default device changed, a USB or
//! Bluetooth device went away, or a resume invalidated the endpoint, every later sound was
//! silent — or went to the old device — and nothing said so: rodio 0.20 hands the stream's error
//! callback to `eprintln!` (to `tracing` only with its `tracing` feature, which this build does
//! not enable), so the error never reached the log either. The application runs for days, and
//! all three happen in days.
//!
//! So the stream is built here, with rodio's own mixer and cpal (rodio's re-export of it: the
//! same version, no new dependency), which is what `OutputStream` does inside, plus an error
//! callback that marks the output as failed. Before each sound [`needs_reopen`] decides: the
//! stream failed, or the system's default output device is another than the default it was
//! opened for. And the manager drops the output after a resume or a new connection
//! (`system_events`), so the next sound opens it afresh. Decoding and mixing stay rodio's.
//!
//! **Why not rodio 0.21, which has this built in** (checked 2026-09-27 against its published
//! source, `src/stream.rs` of 0.21.1): its `OutputStreamBuilder::with_error_callback` takes the
//! same error callback, and `open_default_stream` does the same sample-format dispatch into the
//! same mixer, so [`build`] and [`stream`] below are what 0.21 would replace. It is not taken in
//! this change because 0.21 depends on cpal 0.16 where this build has 0.15.3 — a new major
//! version of the audio layer on both platforms, whose macOS half can only be heard on a Mac —
//! and because its API is not a drop-in (`OutputStreamHandle` and `Sink::try_new` are gone, and
//! its `OutputStream` prints a line to stderr when dropped unless told not to). Its
//! `OutputStream` does not name its device either, so [`needs_reopen`]'s default-device check
//! stays here whichever version builds the stream. TODO.md keeps the move to 0.21.
//!
//! **One known leak, in cpal 0.15.3 and negligible.** Its WASAPI `Stream` skips closing one
//! event handle when dropped after its thread has already ended, which is always so after a
//! device error. So each output dropped after a failure leaves one handle: one per unplugged
//! device or failed resume, not per sound.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rodio::cpal;
use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::dynamic_mixer::{self, DynamicMixer, DynamicMixerController};
use rodio::Source;

/// An open audio output: the stream, the mixer sounds are added to, and whether the stream has
/// reported an error since.
pub(crate) struct Output {
    /// Kept only so the stream lives; dropping it stops the sound.
    _stream: cpal::Stream,
    mixer: Arc<DynamicMixerController<f32>>,
    failed: Arc<AtomicBool>,
    /// The name of the device it was opened on, when the device has one.
    device: Option<String>,
    /// The name of the system's default output device when it was opened — the device itself
    /// when that opened, another when the default would not open and the next one did.
    default_then: Option<String>,
}

/// Whether the output has to be opened again before the next sound: its stream reported an
/// error, or the system's default output device is another than the one that was the default
/// when it was opened. A default that cannot be named now (none, or no name) keeps the output
/// there is: opening again would find nothing better.
pub(crate) fn needs_reopen(failed: bool, default_then: Option<&str>, default_now: Option<&str>) -> bool {
    failed || default_now.is_some_and(|now| default_then != Some(now))
}

impl Output {
    /// Opens the system's default output device, or — as rodio's `try_default` does — the first
    /// other device that opens when the default will not.
    pub(crate) fn open() -> Result<Output, String> {
        let host = cpal::default_host();
        let default = host.default_output_device().ok_or_else(|| "no audio output device".to_string())?;
        let default_then = default.name().ok();
        let mut out = match Output::open_on(&default) {
            Ok(out) => out,
            Err(first) => host
                .output_devices()
                .ok()
                .and_then(|mut all| all.find_map(|d| Output::open_on(&d).ok()))
                .ok_or(first)?,
        };
        out.default_then = default_then;
        Ok(out)
    }

    fn open_on(device: &cpal::Device) -> Result<Output, String> {
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let (mixer, rx) = dynamic_mixer::mixer::<f32>(config.channels(), config.sample_rate().0);
        let failed = Arc::new(AtomicBool::new(false));
        let stream = build(device, &config, rx, failed.clone()).map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Output { _stream: stream, mixer, failed, device: device.name().ok(), default_then: None })
    }

    /// Whether this output has to be opened again before the next sound — see [`needs_reopen`].
    /// Asks the system for its default output device: one query of the audio service, well
    /// under a millisecond to a few, made only when a sound is about to play.
    pub(crate) fn stale(&self) -> bool {
        let default_now = cpal::default_host().default_output_device().and_then(|d| d.name().ok());
        needs_reopen(self.failed.load(Ordering::Acquire), self.default_then.as_deref(), default_now.as_deref())
    }

    /// Plays `source` to its end, mixed with whatever else is playing; returns at once.
    pub(crate) fn play<S>(&self, source: S)
    where
        S: Source + Send + 'static,
        S::Item: rodio::Sample,
        f32: cpal::FromSample<S::Item>,
    {
        self.mixer.add(source.convert_samples::<f32>());
    }

    pub(crate) fn device(&self) -> &str {
        self.device.as_deref().unwrap_or("an unnamed device")
    }
}

/// The stream for `device` in its own sample format, fed from the mixer. The error callback
/// runs on the audio thread: it marks the output failed and writes one line, the first time.
fn build(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    rx: DynamicMixer<f32>,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let cfg = config.config();
    match config.sample_format() {
        cpal::SampleFormat::F32 => stream::<f32>(device, &cfg, rx, failed),
        cpal::SampleFormat::F64 => stream::<f64>(device, &cfg, rx, failed),
        cpal::SampleFormat::I16 => stream::<i16>(device, &cfg, rx, failed),
        cpal::SampleFormat::I32 => stream::<i32>(device, &cfg, rx, failed),
        cpal::SampleFormat::I8 => stream::<i8>(device, &cfg, rx, failed),
        cpal::SampleFormat::U16 => stream::<u16>(device, &cfg, rx, failed),
        cpal::SampleFormat::U32 => stream::<u32>(device, &cfg, rx, failed),
        cpal::SampleFormat::U8 => stream::<u8>(device, &cfg, rx, failed),
        _ => Err(cpal::BuildStreamError::StreamConfigNotSupported),
    }
}

fn stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut rx: DynamicMixer<f32>,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    device.build_output_stream::<T, _, _>(
        config,
        move |data: &mut [T], _| {
            for d in data.iter_mut() {
                *d = rx.next().map(<T as cpal::Sample>::from_sample).unwrap_or(<T as cpal::Sample>::EQUILIBRIUM);
            }
        },
        move |e| {
            if !failed.swap(true, Ordering::AcqRel) {
                crate::logging::line(
                    "sound",
                    &format!("the audio output reported an error ({e}); it is opened again for the next sound"),
                );
            }
        },
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_output_is_opened_again_when_it_failed_or_the_default_moved() {
        // Healthy, on the default: kept.
        assert!(!needs_reopen(false, Some("Speakers"), Some("Speakers")));
        // The stream reported an error — the device went away, a resume invalidated it.
        assert!(needs_reopen(true, Some("Speakers"), Some("Speakers")));
        // The user made another device the default, or the old one went and Windows chose one.
        assert!(needs_reopen(false, Some("Speakers"), Some("Headphones")));
        // The default had no name when it was opened, and the default now has one: opened again.
        assert!(needs_reopen(false, None, Some("Headphones")));
        // No default device to be found now: nothing better to open, so kept.
        assert!(!needs_reopen(false, Some("Speakers"), None));
        assert!(!needs_reopen(false, None, None));
    }
}
