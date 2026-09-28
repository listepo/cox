//! Push-to-talk dictation (P54, A123): local speech-to-text with whisper.cpp.
//! Its own crate under D1 because whisper.cpp is a heavy C++ build (like the
//! grammars in `cox-syntax`); `crates/cox` links it only behind its `voice`
//! feature, off by default. Audio never leaves the process: nothing here
//! opens a socket or writes a file.

mod transcribe;

use std::path::PathBuf;

pub use transcribe::Transcriber;

/// What can go wrong between a model file and a transcript.
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("whisper model not found at {0}; run `cox voice model download <name>`")]
    ModelMissing(PathBuf),
    #[error("{path} is not a whisper ggml model: {reason}")]
    ModelInvalid { path: PathBuf, reason: String },
    #[error("whisper: {0}")]
    Whisper(String),
}
