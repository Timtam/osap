//! `host.sound`'s audio output — and opening it again when the device it was opened on is gone.
//!
//! **The output is rodio's device sink** (`DeviceSinkBuilder`, rodio 0.23 on cpal 0.18): the
//! stream, the sample-format dispatch and the mixer sounds are added to are rodio's, opened on one
//! device with an error callback of ours that marks the output as failed, and with
//! `log_on_drop(false)`, without which every output let go prints a line to stderr. Before each
//! sound [`needs_reopen`] decides: the stream failed, or the system's default output device is
//! another than the default it was opened for. And the manager drops the output after a resume or
//! a new connection (`system_events`), so the next sound opens it afresh. Decoding is rodio's too.
//!
//! **Why not rodio's `open_default_sink`.** It takes no error callback — its own writes to
//! stderr, so a device that went away never reached the log, which is why this file exists — its
//! fallback skips every device whose description names no driver, which is every Core Audio
//! device as cpal 0.18 describes them, and it opens cpal's handle that follows the default device.
//! That handle is also why [`Output::open`] looks the default endpoint up by its id and opens that:
//! on Windows it is activated through `ActivateAudioInterfaceAsync`, which fails for the rest of
//! the process once the system's device notifications have run while any thread is in the
//! multithreaded apartment (cpal#1402, open on 2026-10-25), and the accessibility thread and the
//! WinRT recogniser's are. So the output stays on the endpoint that was the default when it
//! opened, as cpal 0.15.3 had it, and the default-device check is what moves it.
//!
//! **The buffer is the system's** (`BufferSize::Default`), as with cpal 0.15.3. rodio's
//! `from_device` asks for about 50 ms instead, for systems whose default is wrong, which would put
//! some 30 ms more before every cue and, on Core Audio, set the device's own buffer size.
//!
//! **Not every stream error is a failure.** cpal 0.18 sends notices through the same callback: an
//! underrun or a processing overload (`Xrun`; Core Audio reports every overload), a real-time
//! priority the system refused (`RealtimeDenied`), and a stream that followed the default to
//! another device (`DeviceChanged`, which a stream on one endpoint never reports). The sound goes
//! on after each, so the output is kept and nothing is written ([`fails_the_output`]).

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rodio::cpal;
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::{DeviceSinkBuilder, DeviceSinkError, MixerDeviceSink, Source};

use crate::errors::{sound, HostError};

/// A sound file, opened and decoded far enough to play.
pub(crate) type Sound = rodio::Decoder<BufReader<File>>;

/// The sound file at `path`, opened and its header read, or why not, in the words the log has
/// always had: `cannot play '…': …` — `not_found` for a file that is not there, `invalid` for one
/// that does not decode, `failed` for one that is there and cannot be opened (no right to read it,
/// a folder, another program holding it). On the event loop: it reads the file's header and
/// decodes its first block, as rodio's decoder (Symphonia's) does when it is made.
pub(crate) fn open(path: &Path) -> Result<Sound, HostError> {
    let cannot = |reason, e: &dyn std::fmt::Display| HostError::with(reason, format!("cannot play '{}': {e}", path.display()));
    let file = File::open(path).map_err(|e| {
        cannot(if e.kind() == std::io::ErrorKind::NotFound { &sound::NOT_FOUND } else { &sound::FAILED }, &e)
    })?;
    rodio::Decoder::new(BufReader::new(file)).map_err(|e| cannot(&sound::INVALID, &e))
}

/// What `host.sound.play` plays from `path`, a path of the module whose package is `root`: the
/// sound, or why not (`Ok(Err(..))`), answered — or a raise, for a file inside the package that is
/// missing or broken: a sound the module ships is part of it, and a mistyped name in it is a
/// mistake to fix (D3 of the typed-errors plan), as a template's is. Outside the package — an
/// absolute path, or one that climbs out with `..` — the file is the system's or the user's, and
/// its absence is answered.
pub(crate) fn source(root: &Path, path: &Path) -> mlua::Result<Result<Sound, HostError>> {
    match open(path) {
        Err(e) if crate::template_store::in_package(root, path) => Err(mlua::Error::external(e.to_string())),
        other => Ok(other),
    }
}

/// An open audio output: rodio's sink, which holds the stream and the mixer sounds are added
/// to, and whether the stream has reported an error since.
pub(crate) struct Output {
    /// Dropping it stops the sound.
    sink: MixerDeviceSink,
    failed: Arc<AtomicBool>,
    /// The name of the device it was opened on, when the device has one.
    device: Option<String>,
    /// The id of the system's default output device when it was opened (cpal's `DeviceId`, as
    /// text) — the device itself when that opened, another when the default would not open and
    /// the next one did. An id, not a name: two devices of the same name are told apart.
    default_then: Option<String>,
}

/// Whether the output has to be opened again before the next sound: its stream reported an
/// error, or the system's default output device is another than the one that was the default
/// when it was opened. A default that cannot be identified now (none, or no id) keeps the output
/// there is: opening again would find nothing better.
pub(crate) fn needs_reopen(failed: bool, default_then: Option<&str>, default_now: Option<&str>) -> bool {
    failed || default_now.is_some_and(|now| default_then != Some(now))
}

/// Whether a stream error means the output has failed. Not for the notices cpal sends through
/// the same callback while the sound goes on: an underrun or overload, a refused real-time
/// priority, a stream that followed the default elsewhere. A kind cpal adds later counts as a
/// failure: opening again is the safe side.
pub(crate) fn fails_the_output(kind: cpal::ErrorKind) -> bool {
    !matches!(kind, cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied | cpal::ErrorKind::DeviceChanged)
}

/// The audio system's words for why an output would not open: cpal's, which rodio's error
/// carries as its source under a sentence of its own ("Error opening the stream with the OS").
fn said(e: &DeviceSinkError) -> String {
    std::error::Error::source(e).map_or_else(|| e.to_string(), |cause| cause.to_string())
}

/// What `host.sound.play` answers when no output could be opened: `no audio output device`, and
/// the audio system's words — why the default device would not open — as its detail, where it
/// said any; `said` is `None` when the system has no output device at all.
pub(crate) fn no_device(said: Option<String>) -> HostError {
    let e = HostError::of(&sound::NO_DEVICE);
    match said {
        Some(said) => e.with_detail(said),
        None => e,
    }
}

impl Output {
    /// Opens the endpoint that is the system's default output device now, looked up by its id
    /// (see the module doc for why not cpal's handle that follows the default), or the first
    /// other device that opens when it will not — the fallback rodio's `open_default_sink` has,
    /// without its filter. Otherwise the audio system's words for the default device that would
    /// not open, or `None` when there is no output device at all.
    pub(crate) fn open() -> Result<Output, Option<String>> {
        let host = cpal::default_host();
        let default = host.default_output_device().ok_or(None)?;
        let id = default.id().ok();
        let first = id.as_ref().and_then(|id| host.device_by_id(id)).unwrap_or(default);
        let mut out = match Output::open_on(&first) {
            Ok(out) => out,
            Err(words) => host
                .output_devices()
                .ok()
                .and_then(|mut all| all.find_map(|d| Output::open_on(&d).ok()))
                .ok_or(Some(words))?,
        };
        out.default_then = id.map(|id| id.to_string());
        Ok(out)
    }

    /// The sink on `device`, in its default format. The error callback runs on the audio thread:
    /// a failure marks the output failed and writes one line, the first time.
    fn open_on(device: &cpal::Device) -> Result<Output, String> {
        let failed = Arc::new(AtomicBool::new(false));
        let flag = failed.clone();
        let mut sink = DeviceSinkBuilder::from_device(device.clone())
            .map_err(|e| said(&e))?
            .with_buffer_size(cpal::BufferSize::Default)
            .with_error_callback(move |e: cpal::Error| {
                if fails_the_output(e.kind()) && !flag.swap(true, Ordering::AcqRel) {
                    crate::logging::line(
                        "sound",
                        &format!("the audio output reported an error ({e}); it is opened again for the next sound"),
                    );
                }
            })
            .open_stream()
            .map_err(|e| said(&e))?;
        sink.log_on_drop(false);
        let name = device.description().ok().map(|d| d.name().to_owned());
        Ok(Output { sink, failed, device: name, default_then: None })
    }

    /// Whether this output has to be opened again before the next sound — see [`needs_reopen`].
    /// Asks the system for its default output device and that device's id: one query of the
    /// audio service, well under a millisecond to a few, made only when a sound is about to play.
    pub(crate) fn stale(&self) -> bool {
        let default_now = cpal::default_host().default_output_device().and_then(|d| d.id().ok()).map(|id| id.to_string());
        needs_reopen(self.failed.load(Ordering::Acquire), self.default_then.as_deref(), default_now.as_deref())
    }

    /// Plays `source` to its end, mixed with whatever else is playing; returns at once.
    pub(crate) fn play<S: Source + Send + 'static>(&self, source: S) {
        self.sink.mixer().add(source);
    }

    pub(crate) fn device(&self) -> &str {
        self.device.as_deref().unwrap_or("an unnamed device")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing file and one that does not decode: inside the package they raise, with the words
    /// the log always had; outside it they are answered, `not_found` and `invalid`. Nothing is
    /// played: no output is opened here.
    #[test]
    fn a_file_inside_the_package_raises_and_one_outside_is_answered() {
        let dir = std::env::temp_dir().join(format!("host-sound-test-{}", std::process::id()));
        let root = dir.join("module");
        std::fs::create_dir_all(root.join("sounds")).unwrap();
        std::fs::write(root.join("sounds").join("noise.wav"), b"not a sound at all").unwrap();
        std::fs::write(dir.join("elsewhere.wav"), b"not a sound either").unwrap();

        let missing = root.join("sounds").join("gone.wav");
        let raised = source(&root, &missing).err().expect("a missing sound of the package raises");
        assert!(raised.to_string().contains(&format!("cannot play '{}': ", missing.display())), "{raised}");
        assert!(source(&root, &root.join("sounds").join("noise.wav")).is_err(), "a broken one raises");

        let outside = dir.join("gone.wav");
        let e = source(&root, &outside).unwrap().err().expect("answered");
        assert_eq!(e.reason(), &sound::NOT_FOUND);
        assert!(e.message().starts_with(&format!("cannot play '{}': ", outside.display())), "{e}");
        let climbed = root.join("..").join("elsewhere.wav");
        let e = source(&root, &climbed).unwrap().err().expect("answered");
        assert_eq!(e.reason(), &sound::INVALID, "{e}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// No output opened: the message says so once, and the audio system's words, where it said
    /// any, are the detail — not a second copy of the message after it.
    #[test]
    fn no_output_device_is_said_once_and_the_systems_words_are_the_detail() {
        let none = no_device(None);
        assert_eq!((none.reason(), none.message(), none.detail()), (&sound::NO_DEVICE, "no audio output device", None));
        let refused = no_device(Some("The requested device is not available".into()));
        assert_eq!(refused.message(), "no audio output device");
        assert_eq!(refused.detail(), Some("The requested device is not available"));
        assert_eq!(sound::NO_DEVICE.message, "no audio output device", "no `…`: nothing is formatted into it");
    }

    /// The defaults are compared by cpal's device id, as text, so the strings here stand for ids.
    #[test]
    fn the_output_is_opened_again_when_it_failed_or_the_default_moved() {
        let (speakers, headphones) = ("wasapi:{0.0.0.00000000}.{speakers}", "wasapi:{0.0.0.00000000}.{headphones}");
        // Healthy, on the default: kept.
        assert!(!needs_reopen(false, Some(speakers), Some(speakers)));
        // The stream reported an error — the device went away, a resume invalidated it.
        assert!(needs_reopen(true, Some(speakers), Some(speakers)));
        // The user made another device the default, or the old one went and Windows chose one.
        assert!(needs_reopen(false, Some(speakers), Some(headphones)));
        // The default had no id when it was opened, and the default now has one: opened again.
        assert!(needs_reopen(false, None, Some(headphones)));
        // No default device to be found now: nothing better to open, so kept.
        assert!(!needs_reopen(false, Some(speakers), None));
        assert!(!needs_reopen(false, None, None));
    }

    /// cpal's notices keep the output; everything else, a kind it adds later included, fails it.
    #[test]
    fn dropouts_and_refused_priority_are_not_failures() {
        use cpal::ErrorKind::*;
        for kind in [Xrun, RealtimeDenied, DeviceChanged] {
            assert!(!fails_the_output(kind), "{kind:?}");
        }
        for kind in [DeviceNotAvailable, StreamInvalidated, DeviceBusy, BackendError, HostUnavailable, ResourceExhausted, Other] {
            assert!(fails_the_output(kind), "{kind:?}");
        }
    }

    /// The detail of `no audio output device` is cpal's words, not rodio's sentence above them.
    #[test]
    fn the_systems_words_are_cpals_not_rodios() {
        let busy = cpal::Error::with_message(cpal::ErrorKind::DeviceBusy, "Failed to build audio client: in use");
        assert_eq!(said(&DeviceSinkError::BuildError(busy)), "Failed to build audio client: in use");
        let bare = cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable);
        assert_eq!(
            said(&DeviceSinkError::DefaultSinkConfigError(bare)),
            "The requested audio device is not available. It may have been disconnected."
        );
        // No cause under it: rodio's own words are all there is.
        assert_eq!(said(&DeviceSinkError::NoDevice), "Could not find any output device");
    }

    /// Every sound the repository ships decodes to its end — the decoders are Symphonia's since
    /// rodio 0.23 (hound, claxon and lewton before). Nothing is played: no output is opened.
    #[test]
    fn every_shipped_sound_decodes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut stack: Vec<_> = ["modules", "examples", "tools"].iter().map(|d| root.join(d)).collect();
        let mut decoded = 0;
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for path in entries.flatten().map(|e| e.path()) {
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
                if !matches!(ext.as_deref(), Some("wav" | "flac" | "ogg" | "mp3")) {
                    continue;
                }
                let sound = open(&path).unwrap_or_else(|e| panic!("{e}"));
                assert!(sound.channels().get() > 0 && sound.sample_rate().get() > 0, "{}", path.display());
                assert!(sound.count() > 0, "{} has no samples", path.display());
                decoded += 1;
            }
        }
        // examples/sound's ding and VPS Avenger's three: the walk found them.
        assert!(decoded >= 4, "only {decoded} sounds found under {}", root.display());
    }

    /// The `cpal` entry in Cargo.toml is there only for its `realtime` feature, which reaches
    /// rodio's stream only while both name the same cpal. A rodio on another cpal would build
    /// two, and the feature would miss the one that plays: this fails first.
    #[test]
    fn one_cpal_in_the_build() {
        let lock = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock")).unwrap();
        let cpals = lock.lines().filter(|l| *l == "name = \"cpal\"").count();
        assert_eq!(cpals, 1, "Cargo.lock has {cpals} cpal packages: make crates/host/Cargo.toml's cpal the one rodio uses");
    }
}
