//! Monocular depth estimation for focus-based grain and halation.
//!
//! Uses Depth Anything V2 Small (`onnx-community/depth-anything-v2-small`,
//! Apache-2.0) through ONNX Runtime. The model is not shipped; it is downloaded
//! on demand into the user cache. Output is a normalised depth map where 1.0 is
//! near (sharp, in focus) and 0.0 is far (defocused).
//!
//! Approximation: the image is squashed to the model's 518x518 input rather than
//! aspect-preserving; the depth map is then resampled back to image resolution.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::error::{Error, Result};
use crate::image::Image;

/// Model input resolution.
const INPUT_SIZE: u32 = 518;

/// Default model: int8-quantised Depth Anything V2 Small (~27 MB).
const MODEL_URL: &str = "https://huggingface.co/onnx-community/depth-anything-v2-small/resolve/main/onnx/model_quantized.onnx";

/// Cached model file name.
const MODEL_FILE: &str = "depth-anything-v2-small.onnx";

/// ImageNet normalisation used by the model's preprocessor.
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

/// A normalised depth map at image resolution; 255 is near, 0 is far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepthMap {
    width: u32,
    height: u32,
    /// One byte per pixel.
    data: Vec<u8>,
}

impl DepthMap {
    /// Build a depth map from one byte per pixel.
    pub fn from_bytes(width: u32, height: u32, data: Vec<u8>) -> Result<Self> {
        if width == 0 || height == 0 || data.len() != (width * height) as usize {
            return Err(Error::Depth(format!(
                "invalid depth map {width}x{height} with {} samples",
                data.len()
            )));
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Map width.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Map height.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// One byte per pixel.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Expand to RGBA8 (all colour channels carry the depth) for the GPU passes.
    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.data.len() * 4);
        for &d in &self.data {
            out.extend_from_slice(&[d, d, d, 255]);
        }
        out
    }

    /// Convert to an [`Image`] for use as a compute-pass input.
    pub fn to_image(&self) -> Result<Image> {
        Image::from_rgba8(self.width, self.height, self.to_rgba8())
    }
}

/// Depth Anything V2 Small, loaded from an ONNX file.
pub struct DepthModel {
    /// Keeps the process-global ONNX Runtime environment alive.
    _environment: ort::environment::Environment,
    session: ort::session::Session,
    input_name: String,
    output_name: String,
}

impl DepthModel {
    /// Load a model from an ONNX file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let environment = ort::init()
            .build()
            .map_err(|e| Error::Depth(e.to_string()))?;
        let session = ort::session::Session::builder(&environment)
            .map_err(|e| Error::Depth(e.to_string()))?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .map_err(|e| Error::Depth(e.to_string()))?
            .with_intra_threads(4)
            .map_err(|e| Error::Depth(e.to_string()))?
            .commit_from_file(path)
            .map_err(|e| Error::Depth(format!("loading {}: {e}", path.display())))?;

        let input_name = session
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .ok_or_else(|| Error::Depth("model has no inputs".into()))?;
        let output_name = session
            .outputs()
            .first()
            .map(|o| o.name().to_string())
            .ok_or_else(|| Error::Depth("model has no outputs".into()))?;

        Ok(Self {
            _environment: environment,
            session,
            input_name,
            output_name,
        })
    }

    /// The default cache path for the model.
    pub fn default_path() -> PathBuf {
        cache_dir().join(MODEL_FILE)
    }

    /// Load the model from the cache, downloading it first if necessary.
    pub fn load_or_download() -> Result<Self> {
        let path = Self::default_path();
        if !path.exists() {
            download(MODEL_URL, &path)?;
        }
        Self::from_file(&path)
    }

    /// Predict a depth map for `image`, at the image's resolution.
    pub fn predict(&mut self, image: &Image) -> Result<DepthMap> {
        let input = preprocess(image);
        let tensor = ort::value::Tensor::from_array((
            [1i64, 3, INPUT_SIZE as i64, INPUT_SIZE as i64],
            input,
        ))
        .map_err(|e| Error::Depth(e.to_string()))?;

        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => tensor])
            .map_err(|e| Error::Depth(e.to_string()))?;
        let output = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| Error::Depth("model produced no output".into()))?;
        let (_shape, depth) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Depth(e.to_string()))?;

        let (min, max) = depth
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| {
                (lo.min(v), hi.max(v))
            });
        let range = (max - min).max(1e-6);

        // Normalise to 0..=255 and resample to the image size.
        let mut normalized = Vec::with_capacity(depth.len());
        for &v in depth {
            let t = ((v - min) / range).clamp(0.0, 1.0);
            normalized.push((t * 255.0).round() as u8);
        }

        let (w, h) = (image.width(), image.height());
        let src = image::GrayImage::from_raw(INPUT_SIZE, INPUT_SIZE, normalized)
            .ok_or_else(|| Error::Depth("inconsistent depth buffer".into()))?;
        let resized = image::imageops::resize(&src, w, h, image::imageops::FilterType::Triangle);
        DepthMap::from_bytes(w, h, resized.into_raw())
    }
}

/// Resize to the model input and normalise to NCHW.
fn preprocess(image: &Image) -> Vec<f32> {
    let rgba = image::RgbaImage::from_raw(image.width(), image.height(), image.data().to_vec())
        .expect("consistent RGBA buffer");
    let small = image::imageops::resize(
        &rgba,
        INPUT_SIZE,
        INPUT_SIZE,
        image::imageops::FilterType::Triangle,
    );

    let n = (INPUT_SIZE * INPUT_SIZE) as usize;
    let mut out = vec![0.0f32; n * 3];
    for (i, px) in small.pixels().enumerate() {
        let [r, g, b, _] = px.0;
        out[i] = (r as f32 / 255.0 - MEAN[0]) / STD[0];
        out[n + i] = (g as f32 / 255.0 - MEAN[1]) / STD[1];
        out[2 * n + i] = (b as f32 / 255.0 - MEAN[2]) / STD[2];
    }
    out
}

/// The user cache directory for analog models.
///
/// Linux: `$XDG_CACHE_HOME/analog` or `~/.cache/analog`; macOS:
/// `~/Library/Caches/analog`; Windows: `%LOCALAPPDATA%\analog\cache`. Falls back
/// to the temporary directory if no home directory can be determined.
fn cache_dir() -> PathBuf {
    ProjectDirs::from("", "", "analog")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("analog"))
}

/// Download `url` to `path` atomically (via a temporary file).
fn download(url: &str, path: &Path) -> Result<()> {
    log::info!("downloading depth model to {}", path.display());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let mut response = ureq::get(url)
        .call()
        .map_err(|e| Error::Depth(format!("downloading model: {e}")))?;
    let bytes = response
        .body_mut()
        .read_to_vec()
        .map_err(|e| Error::Depth(format!("reading model: {e}")))?;

    let tmp = path.with_extension("onnx.part");
    std::fs::write(&tmp, &bytes).map_err(|source| Error::Io {
        path: tmp.clone(),
        source,
    })?;
    std::fs::rename(&tmp, path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_map_to_rgba8_expands() {
        let map = DepthMap::from_bytes(2, 1, vec![0, 255]).unwrap();
        assert_eq!(map.to_rgba8(), vec![0, 0, 0, 255, 255, 255, 255, 255]);
    }

    #[test]
    fn depth_map_validates_size() {
        assert!(DepthMap::from_bytes(2, 2, vec![0; 3]).is_err());
    }

    #[test]
    fn default_path_is_the_cached_model() {
        let path = DepthModel::default_path();
        assert_eq!(path.file_name().unwrap(), MODEL_FILE);
        assert!(path.is_absolute());
    }

    /// Loads the model from the cache and predicts; skipped when unavailable.
    #[test]
    fn predict_on_synthetic_image() {
        if !DepthModel::default_path().exists() {
            eprintln!("skipping depth test: model not cached");
            return;
        }
        let mut model = match DepthModel::load_or_download() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("skipping depth test: {e}");
                return;
            }
        };
        // A left-dark / right-bright image gives a plausible depth gradient.
        let (w, h) = (64u32, 48u32);
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for _y in 0..h {
            for x in 0..w {
                let v = if x < w / 2 { 20 } else { 235 };
                data.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let img = Image::from_rgba8(w, h, data).unwrap();
        let depth = model.predict(&img).unwrap();
        assert_eq!(depth.width(), w);
        assert_eq!(depth.height(), h);
        assert_eq!(depth.data().len(), (w * h) as usize);
    }
}
