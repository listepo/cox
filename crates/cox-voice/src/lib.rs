//! Push-to-talk dictation (P54, A123): local speech-to-text with whisper.cpp.
//! Its own crate under D1 because whisper.cpp is a heavy C++ build (like the
//! grammars in `cox-syntax`); `crates/cox` links it only behind its `voice`
//! feature, off by default. Audio never leaves the process: nothing here
//! opens a socket or writes a file, and captured samples live only in memory
//! until they are transcribed or dropped.

mod capture;
mod transcribe;

use std::path::PathBuf;

pub use capture::{Recorder, WHISPER_RATE};
pub use transcribe::Transcriber;

/// The pinned model table `cox-vendor whisper-models` writes (T54.1): per
/// model its name, file, download URL at a fixed commit, SHA-256 and size.
/// `cox voice model` reads it; this crate never downloads anything.
pub const MODELS_JSON: &str = include_str!("../data/whisper-models.json");

/// What can go wrong between a model file and a transcript.
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("whisper model not found at {0}; run `cox voice model download <name>`")]
    ModelMissing(PathBuf),
    #[error("{path} is not a whisper ggml model: {reason}")]
    ModelInvalid { path: PathBuf, reason: String },
    #[error("whisper: {0}")]
    Whisper(String),
    #[error(
        "no microphone found; check that one is connected and that this terminal may use it (macOS: System Settings > Privacy & Security > Microphone)"
    )]
    NoInputDevice,
    #[error(
        "microphone: {0}; if the OS denied access, allow this terminal to use the microphone (macOS: System Settings > Privacy & Security > Microphone)"
    )]
    Stream(String),
}
