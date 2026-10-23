//! Calibration metrics for the grain block (Phase 0).
//!
//! These tests do not assert visual thresholds yet; they print the numbers the
//! fine-grain work is measured against. Run with:
//!
//! ```sh
//! cargo test -p analog-core --test grain_metrics -- --nocapture
//! ```
//!
//! On a machine without a GPU the test skips like the in-crate GPU tests.

use analog_core::{GpuContext, GrainParams, GrainType, Image, Recipe, Renderer};

const SEED: u64 = 0x1234_5678_9abc_def0;

fn ctx_or_skip() -> Option<GpuContext> {
    match GpuContext::new_blocking() {
        Ok(ctx) => Some(ctx),
        Err(e) => {
            eprintln!("skipping GPU test: {e}");
            None
        }
    }
}

fn flat_image(width: u32, height: u32, value: u8) -> Image {
    Image::from_rgba8(width, height, vec![value; (width * height * 4) as usize]).unwrap()
}

fn wedge_image(width: u32, height: u32) -> Image {
    let levels = [0.1_f32, 0.3, 0.5, 0.7, 0.9];
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let band = ((y as usize * levels.len()) / height as usize).min(levels.len() - 1);
        let v = (levels[band] * 255.0).round() as u8;
        for _ in 0..width {
            data.extend_from_slice(&[v, v, v, 255]);
        }
    }
    Image::from_rgba8(width, height, data).unwrap()
}

/// Fine, small, low softness: the closest current approximation of the target
/// look before the new generator lands.
fn metric_recipe() -> Recipe {
    Recipe {
        grain_seed: SEED,
        grain: GrainParams {
            grain_type: GrainType::Fine,
            intensity: 45.0,
            grain_amount: 15.0,
            roughness: 5.0,
            grain_color: 16.0,
            luminance_highlights: 25.0,
            luminance_mids: 38.0,
            luminance_shadows: 64.0,
            ..Default::default()
        },
        ..Recipe::default()
    }
}

fn luma8(image: &Image) -> Vec<f32> {
    image
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32)
        .collect()
}

/// Mean channel spread per pixel, in 0..255. Zero for a neutral image.
fn chroma_mean(image: &Image) -> f32 {
    let pixels = image.data().as_chunks::<4>().0;
    let sum: f32 = pixels
        .iter()
        .map(|p| {
            let max = p[0].max(p[1]).max(p[2]) as f32;
            let min = p[0].min(p[1]).min(p[2]) as f32;
            max - min
        })
        .sum();
    sum / pixels.len() as f32
}

fn mean_luma(image: &Image) -> f32 {
    luma8(image).iter().sum::<f32>() / (image.width() * image.height()) as f32
}

/// Subtract the region mean. On a flat patch every remaining variation is
/// grain; unlike a local high-pass this keeps large, blurred features.
fn residual(luma: &[f32], width: u32, y0: usize, y1: usize) -> Vec<f32> {
    let w = width as usize;
    let mut values = Vec::with_capacity(w * y1.saturating_sub(y0));
    for y in y0..y1 {
        values.extend_from_slice(&luma[y * w..(y + 1) * w]);
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    for v in &mut values {
        *v -= mean;
    }
    values
}

fn band_bounds(height: u32, band: usize, bands: usize) -> (usize, usize) {
    let y0 = (height as usize * band) / bands + 2;
    let y1 = ((height as usize * (band + 1)) / bands)
        .saturating_sub(2)
        .max(y0);
    (y0, y1)
}

fn rms(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    (values.iter().map(|v| v * v).sum::<f32>() / values.len() as f32).sqrt()
}

/// First lag where the horizontal autocorrelation falls to 0.5, linearly
/// interpolated. Small values mean fine grain; large values mean blobs.
fn autocorr_half_width(hp: &[f32], width: u32, height: u32) -> f32 {
    let (w, h) = (width as i32, height as i32);
    let max_lag = 32i32;
    let mut corr = Vec::with_capacity(max_lag as usize + 1);
    for lag in 0..=max_lag {
        let mut sum = 0.0;
        let mut count = 0.0;
        for y in 1..h - 1 {
            for x in 1..w - 1 - lag {
                let a = hp[(y * w + x) as usize];
                let b = hp[(y * w + x + lag) as usize];
                sum += a * b;
                count += 1.0;
            }
        }
        corr.push(if count > 0.0 { sum / count } else { 0.0 });
    }
    let zero = corr[0];
    if zero <= f32::EPSILON {
        return 0.0;
    }
    for lag in 1..=max_lag as usize {
        let prev = corr[lag - 1] / zero;
        let cur = corr[lag] / zero;
        if cur <= 0.5 {
            let t = (prev - 0.5) / (prev - cur).max(f32::EPSILON);
            return (lag - 1) as f32 + t;
        }
    }
    max_lag as f32
}

fn band_metrics(luma: &[f32], width: u32, height: u32, band: usize, bands: usize) -> (f32, f32) {
    let (y0, y1) = band_bounds(height, band, bands);
    let values = residual(luma, width, y0, y1);
    (
        rms(&values),
        autocorr_half_width(&values, width, (y1 - y0) as u32),
    )
}

/// Box-downsample an 8-bit image by an integer factor.
fn downsample(image: &Image, factor: u32) -> Image {
    let (w, h) = (image.width() / factor, image.height() / factor);
    let src = image.data();
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            for c in 0..4 {
                let mut sum = 0u32;
                for dy in 0..factor {
                    for dx in 0..factor {
                        let sx = x * factor + dx;
                        let sy = y * factor + dy;
                        sum += src[((sy * image.width() + sx) * 4 + c) as usize] as u32;
                    }
                }
                out[((y * w + x) * 4 + c) as usize] = (sum / (factor * factor)) as u8;
            }
        }
    }
    Image::from_rgba8(w, h, out).unwrap()
}

/// Coarse reference: the old default type and slider values under the new
/// mapping, kept so the size/softness change stays measurable.
fn legacy_recipe() -> Recipe {
    Recipe {
        grain_seed: SEED,
        grain: GrainParams {
            grain_type: GrainType::Coarse,
            intensity: 50.0,
            grain_amount: 50.0,
            roughness: 50.0,
            grain_color: 50.0,
            luminance_highlights: 50.0,
            luminance_mids: 50.0,
            luminance_shadows: 50.0,
            ..Default::default()
        },
        ..Recipe::default()
    }
}

/// Metrics for one recipe on a flat patch and a five-step wedge.
#[derive(Debug, Clone, Copy)]
struct Report {
    rms: f32,
    half_width: f32,
    /// Residual RMS per wedge band, from luma 0.1 to 0.9.
    bands: [f32; 5],
}

#[test]
fn report_grain_metrics() {
    let Some(ctx) = ctx_or_skip() else { return };
    let renderer = Renderer::new();

    report_recipe(&ctx, &renderer, "coarse reference", &legacy_recipe());
    let defaults = report_recipe(&ctx, &renderer, "shipped defaults", &Recipe::default());
    report_recipe(&ctx, &renderer, "fine calibration", &metric_recipe());

    assert!(
        (0.3..1.5).contains(&defaults.half_width),
        "default grain half-width {:.2} px is outside the fine range",
        defaults.half_width
    );
    assert!(
        defaults.rms > 0.5,
        "default grain is too weak: {:.3}/255",
        defaults.rms
    );
    assert!(
        defaults.bands[4] < defaults.bands[2] * 0.7,
        "highlights should carry less grain than mids: {:?}",
        defaults.bands
    );

    // Preview versus export. Grain below one processing pixel aliases, so this
    // checks that the overall amplitude stays in the same ballpark rather than
    // expecting pixel-identical fields.
    let (pw, ph) = (600, 400);
    let (ew, eh) = (1800, 1200);
    let recipe = metric_recipe();
    let flat = flat_image(pw, ph, 128);
    let preview = renderer
        .render_with_reference(&ctx, &flat, &recipe, None, Some(ew))
        .unwrap();
    let export = renderer
        .render(&ctx, &flat_image(ew, eh, 128), &recipe, None)
        .unwrap();
    let export_small = downsample(&export, 3);
    let preview_luma = luma8(&preview);
    let export_luma = luma8(&export_small);
    let preview_res = residual(&preview_luma, pw, 2, ph as usize - 2);
    let export_res = residual(&export_luma, pw, 2, ph as usize - 2);
    let diff: f32 = preview_luma
        .iter()
        .zip(&export_luma)
        .map(|(a, b)| (a - b).abs())
        .sum::<f32>()
        / preview_luma.len() as f32;
    println!(
        "preview rms = {:.2}/255, half-width = {:.2} px",
        rms(&preview_res),
        autocorr_half_width(&preview_res, pw, ph - 4)
    );
    println!(
        "export/3 rms = {:.2}/255, half-width = {:.2} px",
        rms(&export_res),
        autocorr_half_width(&export_res, pw, ph - 4)
    );
    println!("preview vs downscaled export: mean |diff| = {diff:.3}/255");
    assert!(
        diff < 5.0,
        "preview and downscaled export differ by {diff:.2}/255"
    );

    // Chroma: Color 16 must stay subtle and the deviations must not shift
    // local luminance.
    let mut low_color = metric_recipe();
    low_color.grain.grain_color = 16.0;
    let mut high_color = metric_recipe();
    high_color.grain.grain_color = 80.0;
    let low_image = renderer
        .render(&ctx, &flat_image(pw, ph, 128), &low_color, None)
        .unwrap();
    let high_image = renderer
        .render(&ctx, &flat_image(pw, ph, 128), &high_color, None)
        .unwrap();
    let low_chroma = chroma_mean(&low_image);
    let high_chroma = chroma_mean(&high_image);
    let luma_shift = (mean_luma(&high_image) - 128.0).abs();
    println!(
        "chroma at Color 16 = {low_chroma:.3}/255, at Color 80 = {high_chroma:.3}/255, \
         luma shift at Color 80 = {luma_shift:.3}/255"
    );
    assert!(
        low_chroma < high_chroma * 0.5,
        "Color 16 should be much less chromatic than Color 80"
    );
    assert!(
        high_chroma > 0.1,
        "Color 80 should still produce visible chroma"
    );
    assert!(
        luma_shift < 1.0,
        "chroma deviations shifted luminance by {luma_shift:.2}/255"
    );
}

fn report_recipe(ctx: &GpuContext, renderer: &Renderer, label: &str, recipe: &Recipe) -> Report {
    let (w, h) = (600, 400);
    let flat = flat_image(w, h, 128);
    let out = renderer.render(ctx, &flat, recipe, None).unwrap();
    let luma = luma8(&out);
    let flat_residual = residual(&luma, w, 2, h as usize - 2);
    let rms_value = rms(&flat_residual);
    let half_width = autocorr_half_width(&flat_residual, w, h - 4);
    println!("{label}: rms = {rms_value:.2}/255, autocorr half-width = {half_width:.2} px");

    let wedge = wedge_image(w, h);
    let out = renderer.render(ctx, &wedge, recipe, None).unwrap();
    let wedge_luma = luma8(&out);
    let mut bands = [0.0; 5];
    for (band, level) in [0.1_f32, 0.3, 0.5, 0.7, 0.9].iter().enumerate() {
        let (band_rms, band_width) = band_metrics(&wedge_luma, w, h, band, 5);
        bands[band] = band_rms;
        println!(
            "  wedge luma {level:.1}: rms = {band_rms:.2}/255, half-width = {band_width:.2} px"
        );
    }
    Report {
        rms: rms_value,
        half_width,
        bands,
    }
}
