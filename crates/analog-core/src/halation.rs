//! The Halation block.
//!
//! A Kawase-style bloom: bright extraction with a soft knee, a box-filter
//! downsample chain, tent upsample with additive accumulation, and a
//! hue-tinted composite (70 percent additive, 30 percent screen).
//!
//! Approximations, documented in `docs/architecture.md`: the mip chain is a
//! fixed 4 levels; `halation_radius` scales the accumulation weight rather than
//! an exact kernel radius; and depth-aware background handling
//! (`halation_background_gain` / `halation_background_focus`) applies when a
//! depth map is present.

use crate::error::Result;
use crate::gpu::GpuContext;
use crate::image::PixelFormat;
use crate::params::HalationParams;
use crate::passes::{GpuImage, ImagePassPair, LazyPass};

/// Maximum number of downsample levels (1/2 .. 1/64).
const MAX_MIP_LEVELS: usize = 6;

/// Soft knee width for the bright extraction, in normalised units.
const KNEE: f32 = 0.15;

/// The halation pipeline, reusable across images.
pub struct HalationPass {
    extract: ImagePassPair,
    downsample: LazyPass,
    upsample_add: LazyPass,
    composite: ImagePassPair,
}

impl HalationPass {
    /// Create the (lazily compiled) halation pipeline.
    pub fn new() -> Self {
        Self {
            extract: ImagePassPair::new(
                "halation.extract",
                include_str!("../shaders/halation_extract.wgsl"),
            ),
            downsample: LazyPass::new(
                "halation.downsample",
                include_str!("../shaders/halation_downsample.wgsl"),
            ),
            upsample_add: LazyPass::new(
                "halation.upsample_add",
                include_str!("../shaders/halation_upsample_add.wgsl"),
            ),
            composite: ImagePassPair::new(
                "halation.composite",
                include_str!("../shaders/halation_composite.wgsl"),
            ),
        }
    }

    /// Apply halation to `image`.
    ///
    /// When `depth` is `Some`, `halation_background_focus` and
    /// `halation_background_gain` suppress the glow on the in-focus subject and
    /// boost it in the defocused background; when `None`, both are ignored.
    pub fn run(
        &self,
        ctx: &GpuContext,
        image: &GpuImage,
        p: &HalationParams,
        depth: Option<&GpuImage>,
    ) -> Result<GpuImage> {
        let (w, h) = (image.width(), image.height());

        // 1. Bright extraction at full resolution (bloom is 8-bit).
        let extract_u = [w, h, p.halation_threshold.to_bits(), KNEE.to_bits()];
        let mut levels: Vec<GpuImage> = vec![self.extract.select(ctx, image.format()).run_gpu(
            ctx,
            &[image],
            w,
            h,
            PixelFormat::Rgba8,
            bytemuck::cast_slice(&extract_u),
        )?];

        // 2. Box downsample chain. The number of levels grows with the radius,
        // so a larger radius produces a wider bloom.
        let levels_used =
            1 + ((p.halation_radius / 100.0) * (MAX_MIP_LEVELS - 1) as f32).round() as usize;
        let levels_used = levels_used.clamp(1, MAX_MIP_LEVELS);
        for _ in 0..levels_used {
            let prev = levels.last().expect("at least one level");
            let (pw, ph) = (prev.width(), prev.height());
            if pw <= 1 && ph <= 1 {
                break;
            }
            let (ow, oh) = (pw.div_ceil(2), ph.div_ceil(2));
            let u = [pw, ph, ow, oh];
            let down = self.downsample.get(ctx).run_gpu(
                ctx,
                &[prev],
                ow,
                oh,
                PixelFormat::Rgba8,
                bytemuck::cast_slice(&u),
            )?;
            levels.push(down);
        }

        // 3. Tent upsample with additive accumulation, coarsest to finest.
        let weight: f32 = 0.8;
        let mut acc = levels.pop().expect("at least one level");
        while let Some(fine) = levels.pop() {
            let (ow, oh) = (fine.width(), fine.height());
            let (cw, ch) = (acc.width(), acc.height());
            let u = [ow, oh, cw, ch, weight.to_bits(), 0u32, 0u32, 0u32];
            acc = self.upsample_add.get(ctx).run_gpu(
                ctx,
                &[&fine, &acc],
                ow,
                oh,
                PixelFormat::Rgba8,
                bytemuck::cast_slice(&u),
            )?;
        }

        // 4. Composite onto the image.
        let intensity = (p.halation_intensity + 0.5 * p.light_spill_intensity) / 100.0;
        let [tr, tg, tb] = tint_rgb(p);
        let background_focus = if depth.is_some() {
            p.halation_background_focus
        } else {
            0.0
        };
        let u = [
            w,
            h,
            intensity.to_bits(),
            tr.to_bits(),
            tg.to_bits(),
            tb.to_bits(),
            background_focus.to_bits(),
            p.halation_background_gain.to_bits(),
            0u32,
            0u32,
            0u32,
            0u32,
        ];
        let dummy = GpuImage::upload(
            ctx,
            &crate::image::Image::from_rgba8(1, 1, vec![255, 255, 255, 255])?,
        )?;
        let depth_ref = depth.unwrap_or(&dummy);
        self.composite.select(ctx, image.format()).run_gpu(
            ctx,
            &[image, &acc, depth_ref],
            w,
            h,
            image.format(),
            bytemuck::cast_slice(&u),
        )
    }
}

impl Default for HalationPass {
    fn default() -> Self {
        Self::new()
    }
}

/// The bloom tint as RGB multipliers, from `halation_color_tone`,
/// `halation_color_shift` and `halation_color_intensity`.
///
/// Exposed so the GUI can show a gradient swatch that matches the render.
pub fn tint_rgb(p: &HalationParams) -> [f32; 3] {
    // `color_tone` sets the base hue on the yellow-to-red ramp; `color_shift`
    // biases it (50 is neutral, higher is redder, lower is yellower).
    let shift = (p.halation_color_shift - 50.0) / 100.0;
    let t = (p.halation_color_tone / 100.0 + shift).clamp(0.0, 1.0);
    let yellow = [1.0f32, 0.80, 0.35];
    let red = [1.0f32, 0.30, 0.12];
    let hue = [
        yellow[0] + (red[0] - yellow[0]) * t,
        yellow[1] + (red[1] - yellow[1]) * t,
        yellow[2] + (red[2] - yellow[2]) * t,
    ];
    let sat = (p.halation_color_intensity / 100.0).clamp(0.0, 1.0);
    [
        1.0 + (hue[0] - 1.0) * sat,
        1.0 + (hue[1] - 1.0) * sat,
        1.0 + (hue[2] - 1.0) * sat,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Recipe;
    use crate::image::Image;
    use crate::params::HalationParams;

    fn ctx_or_skip() -> Option<GpuContext> {
        match GpuContext::new_blocking() {
            Ok(ctx) => Some(ctx),
            Err(e) => {
                eprintln!("skipping GPU test: {e}");
                None
            }
        }
    }

    fn bright_image(w: u32, h: u32) -> Image {
        // Mid-grey with a bright hotspot in the middle.
        let mut data = vec![80u8; (w * h * 4) as usize];
        for y in h / 3..2 * h / 3 {
            for x in w / 3..2 * w / 3 {
                let i = ((y * w + x) * 4) as usize;
                data[i] = 255;
                data[i + 1] = 255;
                data[i + 2] = 255;
            }
        }
        for a in data.chunks_exact_mut(4) {
            a[3] = 255;
        }
        Image::from_rgba8(w, h, data).unwrap()
    }

    fn halation_recipe(intensity: f32, seed: u64) -> Recipe {
        Recipe {
            grain_seed: seed,
            grain: crate::params::GrainParams {
                intensity: 0.0,
                ..Default::default()
            },
            halation: HalationParams {
                halation_intensity: intensity,
                ..Default::default()
            },
            ..Recipe::default()
        }
    }

    #[test]
    fn halation_is_deterministic() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = bright_image(64, 48);
        let recipe = halation_recipe(100.0, 7);
        let a = crate::render::render(&ctx, &img, &recipe).unwrap();
        let b = crate::render::render(&ctx, &img, &recipe).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn halation_brightens_near_the_hotspot() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = bright_image(64, 48);
        let out = crate::render::render(&ctx, &img, &halation_recipe(100.0, 0)).unwrap();
        // A pixel just outside the hotspot should gain energy from the glow.
        let w = img.width();
        let probe = ((img.height() / 2 * w + (w / 3 - 2)) * 4) as usize;
        assert!(
            out.data()[probe] > img.data()[probe],
            "glow should brighten pixels near the hotspot"
        );
        // Alpha preserved.
        for (i, px) in out.data().chunks_exact(4).enumerate() {
            assert_eq!(px[3], img.data()[i * 4 + 3]);
        }
    }

    #[test]
    fn zero_intensity_is_identity() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = bright_image(32, 24);
        let out = crate::render::render(&ctx, &img, &halation_recipe(0.0, 0)).unwrap();
        assert_eq!(out, img);
    }

    #[test]
    fn color_shift_changes_the_tint() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = bright_image(64, 48);
        let mut yellow = halation_recipe(100.0, 0);
        yellow.halation.halation_color_shift = 0.0;
        let mut red = halation_recipe(100.0, 0);
        red.halation.halation_color_shift = 100.0;
        let a = crate::render::render(&ctx, &img, &yellow).unwrap();
        let b = crate::render::render(&ctx, &img, &red).unwrap();
        assert_ne!(a, b, "color shift should change the halation tint");
    }
}
