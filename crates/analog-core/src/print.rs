//! The Print Simulation block.
//!
//! A per-paper tone map (shadow floor, S-curve, highlight shoulder, paper tint)
//! blended by `print_simulation`.

use crate::error::Result;
use crate::gpu::GpuContext;
use crate::params::{PaperType, PrintParams};
use crate::passes::{GpuImage, ImagePassPair};

/// A compiled print-simulation pass, reusable across images.
pub struct PrintPass {
    pass: ImagePassPair,
}

impl PrintPass {
    /// Create the (lazily compiled) print-simulation pass.
    pub fn new() -> Self {
        Self {
            pass: ImagePassPair::new("print", include_str!("../shaders/print.wgsl")),
        }
    }

    /// Apply print simulation to `image`.
    pub fn run(&self, ctx: &GpuContext, image: &GpuImage, p: &PrintParams) -> Result<GpuImage> {
        let paper_type = match p.print_sim_paper_type {
            PaperType::Matte => 0u32,
            PaperType::Neutral => 1,
            PaperType::Glossy => 2,
        };
        let u = [
            image.width(),
            image.height(),
            p.print_simulation.to_bits(),
            paper_type,
            p.print_sim_paper_intensity.to_bits(),
            p.print_sim_highlight_rolloff.to_bits(),
            p.print_sim_paper_tint.to_bits(),
            0u32,
        ];
        self.pass.select(ctx, image.format()).run_gpu(
            ctx,
            &[image],
            image.width(),
            image.height(),
            image.format(),
            bytemuck::cast_slice(&u),
        )
    }
}

impl Default for PrintPass {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Recipe;
    use crate::image::Image;
    use crate::params::{GrainParams, PaperType, PrintParams};

    fn ctx_or_skip() -> Option<GpuContext> {
        match GpuContext::new_blocking() {
            Ok(ctx) => Some(ctx),
            Err(e) => {
                eprintln!("skipping GPU test: {e}");
                None
            }
        }
    }

    fn ramp_image(w: u32, h: u32) -> Image {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let v = ((x + y) % 256) as u8;
                data.extend_from_slice(&[v, v, v, 255]);
            }
        }
        Image::from_rgba8(w, h, data).unwrap()
    }

    fn print_recipe(sim: f32, paper: PaperType) -> Recipe {
        Recipe {
            grain: GrainParams {
                intensity: 0.0,
                ..Default::default()
            },
            print: PrintParams {
                print_simulation: sim,
                print_sim_paper_type: paper,
                print_sim_paper_intensity: 100.0,
                ..Default::default()
            },
            ..Recipe::default()
        }
    }

    #[test]
    fn matte_raises_the_black_point() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = ramp_image(32, 32);
        let out =
            crate::render::render(&ctx, &img, &print_recipe(100.0, PaperType::Matte)).unwrap();
        // The darkest pixel should be lifted to roughly the matte floor.
        let min = out.data().chunks_exact(4).map(|p| p[0]).min().unwrap();
        assert!(min >= 20, "matte floor should raise blacks, got min {min}");
    }

    #[test]
    fn glossy_keeps_blacks_dark() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = ramp_image(32, 32);
        let out =
            crate::render::render(&ctx, &img, &print_recipe(100.0, PaperType::Glossy)).unwrap();
        let min = out.data().chunks_exact(4).map(|p| p[0]).min().unwrap();
        assert!(min <= 8, "glossy should keep blacks near zero, got {min}");
    }

    #[test]
    fn zero_simulation_is_identity() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = ramp_image(32, 32);
        let out = crate::render::render(&ctx, &img, &print_recipe(0.0, PaperType::Matte)).unwrap();
        assert_eq!(out, img);
    }

    #[test]
    fn is_deterministic() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = ramp_image(32, 32);
        let recipe = print_recipe(80.0, PaperType::Neutral);
        let a = crate::render::render(&ctx, &img, &recipe).unwrap();
        let b = crate::render::render(&ctx, &img, &recipe).unwrap();
        assert_eq!(a, b);
    }
}
