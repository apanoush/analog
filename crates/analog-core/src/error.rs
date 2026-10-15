//! Crate error type.

use std::path::PathBuf;

/// Errors produced by the `analog-core` library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Reading or writing an image file failed.
    #[error("image i/o error for {path}: {source}")]
    ImageIo {
        path: PathBuf,
        #[source]
        source: image::ImageError,
    },

    /// The image could not be decoded or encoded by any supported codec.
    #[error("unsupported or corrupt image: {0}")]
    Image(String),

    /// A file path had no usable extension to infer a format from.
    #[error("cannot infer image format from path: {0}")]
    UnknownFormat(PathBuf),

    /// Generic I/O failure not tied to the image codecs.
    #[error("i/o error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// No usable GPU adapter was found for the requested backends.
    #[error("no compatible GPU adapter found: {0}")]
    NoAdapter(String),

    /// The GPU device could not be created, or a pipeline failed validation.
    #[error("gpu error: {0}")]
    Gpu(String),

    /// A recipe requests an effect that is not implemented yet.
    #[error("unsupported: {0}")]
    Unsupported(String),

    /// Depth model download, load or inference failed.
    #[error("depth model error: {0}")]
    Depth(String),

    /// A recipe could not be parsed or is malformed.
    #[error("recipe error: {0}")]
    Recipe(String),
}

/// Convenience result alias for the crate.
pub type Result<T> = std::result::Result<T, Error>;
