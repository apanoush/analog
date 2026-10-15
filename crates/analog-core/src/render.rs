//! Top-level render orchestration.
//!
//! Applies the four effect blocks in the documented order: Film Color Shift,
//! Grain, Halation, Print Simulation. The whole chain runs on the GPU: the input
//! is uploaded once, passes hand [`GpuImage`] buffers to each other, and the
//! result is read back once. Earlier versions read back to the CPU after every
//! pass, which was the main cause of slow previews.
//!
//! [`Renderer`] holds the (lazily compiled) passes so interactive use does not
//! recompile shaders.

use crate::Result;
use crate::depth::DepthMap;
use crate::film::FilmPass;
use crate::gpu::GpuContext;
use crate::grain::GrainPass;
use crate::halation::HalationPass;
use crate::image::Image;
use crate::passes::GpuImage;
use crate::print::PrintPass;
use crate::recipe::Recipe;

/// The render pipeline, reusable across images and recipes.
pub struct Renderer {
    film: FilmPass,
    grain: GrainPass,
    halation: HalationPass,
    print: PrintPass,
}

impl Renderer {
    /// Create a renderer. Passes compile lazily on first use.
    pub fn new() -> Self {
        Self {
            film: FilmPass::new(),
            grain: GrainPass::new(),
            halation: HalationPass::new(),
            print: PrintPass::new(),
        }
    }

    /// Render `image` through `recipe` on the GPU.
    ///
    /// `depth`, when provided, drives the depth-dependent controls
    /// (`grain.depth_of_field_intensity`, `halation.halation_background_*`).
    pub fn render(
        &self,
        ctx: &GpuContext,
        image: &Image,
        recipe: &Recipe,
        depth: Option<&DepthMap>,
    ) -> Result<Image> {
        let recipe = recipe.clone().clamped();
        if recipe.is_noop() {
            return Ok(image.clone());
        }

        let mut out = GpuImage::upload(ctx, image)?;
        let depth_gpu = match depth {
            Some(d) => Some(GpuImage::upload(ctx, &d.to_image()?)?),
            None => None,
        };

        if recipe.film.film_stock_enabled && recipe.film.film_stock_intensity > 0.0 {
            out = self.film.run(ctx, &out, &recipe.film)?;
        }
        if recipe.grain.intensity > 0.0 {
            out = self.grain.run(
                ctx,
                &out,
                &recipe.grain,
                recipe.grain_seed,
                depth_gpu.as_ref(),
            )?;
        }
        if recipe.halation.halation_intensity > 0.0 || recipe.halation.light_spill_intensity > 0.0 {
            out = self
                .halation
                .run(ctx, &out, &recipe.halation, depth_gpu.as_ref())?;
        }
        if recipe.print.print_simulation > 0.0 {
            out = self.print.run(ctx, &out, &recipe.print)?;
        }

        out.read(ctx)
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Render `image` through `recipe` without a depth map, for one-shot use.
pub fn render(ctx: &GpuContext, image: &Image, recipe: &Recipe) -> Result<Image> {
    Renderer::new().render(ctx, image, recipe, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::PixelFormat;
    use crate::params::GrainParams;

    fn ctx_or_skip() -> Option<GpuContext> {
        match GpuContext::new_blocking() {
            Ok(ctx) => Some(ctx),
            Err(e) => {
                eprintln!("skipping GPU test: {e}");
                None
            }
        }
    }

    #[test]
    fn renders_16_bit_and_preserves_depth() {
        let Some(ctx) = ctx_or_skip() else { return };
        let data: Vec<u16> = (0..32 * 32 * 4)
            .map(|i| ((i * 1234) % 65536) as u16)
            .collect();
        let img = Image::from_rgba16(32, 32, data).unwrap();
        let recipe = Recipe {
            grain: GrainParams {
                intensity: 60.0,
                ..Default::default()
            },
            ..Recipe::default()
        };
        let out = Renderer::new().render(&ctx, &img, &recipe, None).unwrap();
        assert_eq!(out.format(), PixelFormat::Rgba16);
        assert_eq!(out.width(), 32);
        assert_ne!(out, img, "16-bit grain should alter pixels");
    }

    #[test]
    fn eight_bit_input_stays_eight_bit() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = Image::from_rgba8(16, 16, vec![120; 16 * 16 * 4]).unwrap();
        let out = render(&ctx, &img, &Recipe::default()).unwrap();
        assert_eq!(out.format(), PixelFormat::Rgba8);
    }
}
