//! The Film Color Shift block.
//!
//! A parametric film emulation. Each stock is described by a small
//! [`Profile`] (lift/gain/gamma, saturation, shadow and highlight tint, and for
//! black-and-white stocks a channel mixer and paper tone). No third-party LUTs
//! are used; the profile values are our own approximations of each stock's
//! character, documented as such.
//!
//! Deferred: content-aware hue-cluster narrowing and baked 33-cubed LUTs. See
//! `docs/architecture.md`.

use crate::error::Result;
use crate::gpu::GpuContext;
use crate::params::{FilmParams, FilmStock};
use crate::passes::{GpuImage, ImagePassPair};

/// A compiled film-color pass, reusable across images.
pub struct FilmPass {
    pass: ImagePassPair,
}

impl FilmPass {
    /// Create the (lazily compiled) film-color pass.
    pub fn new() -> Self {
        Self {
            pass: ImagePassPair::new("film", include_str!("../shaders/film.wgsl")),
        }
    }

    /// Apply the film look to `image`.
    pub fn run(&self, ctx: &GpuContext, image: &GpuImage, p: &FilmParams) -> Result<GpuImage> {
        let prof = profile(p.film_stock);
        let u = [
            image.width(),
            image.height(),
            p.film_stock_intensity.to_bits(),
            u32::from(prof.is_bw),
            prof.sat.to_bits(),
            prof.gain[0].to_bits(),
            prof.gain[1].to_bits(),
            prof.gain[2].to_bits(),
            prof.lift[0].to_bits(),
            prof.lift[1].to_bits(),
            prof.lift[2].to_bits(),
            prof.gamma.to_bits(),
            prof.shadow[0].to_bits(),
            prof.shadow[1].to_bits(),
            prof.shadow[2].to_bits(),
            prof.highlight[0].to_bits(),
            prof.highlight[1].to_bits(),
            prof.highlight[2].to_bits(),
            prof.bw[0].to_bits(),
            prof.bw[1].to_bits(),
            prof.bw[2].to_bits(),
            prof.tone[0].to_bits(),
            prof.tone[1].to_bits(),
            prof.tone[2].to_bits(),
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

impl Default for FilmPass {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-stock parametric description.
#[derive(Debug, Clone, Copy)]
struct Profile {
    is_bw: bool,
    sat: f32,
    gain: [f32; 3],
    lift: [f32; 3],
    gamma: f32,
    shadow: [f32; 3],
    highlight: [f32; 3],
    /// Black-and-white channel mixer weights.
    bw: [f32; 3],
    /// Black-and-white paper tone.
    tone: [f32; 3],
}

impl Profile {
    /// An identity profile: no color change.
    fn neutral() -> Self {
        Self {
            is_bw: false,
            sat: 1.0,
            gain: [1.0, 1.0, 1.0],
            lift: [0.0, 0.0, 0.0],
            gamma: 1.0,
            shadow: [0.0, 0.0, 0.0],
            highlight: [0.0, 0.0, 0.0],
            bw: [0.30, 0.59, 0.11],
            tone: [1.0, 1.0, 1.0],
        }
    }

    /// A black-and-white profile with a channel mixer and paper tone.
    fn bw(bw: [f32; 3], tone: [f32; 3]) -> Self {
        Self {
            is_bw: true,
            bw,
            tone,
            ..Self::neutral()
        }
    }
}

/// Our approximation of each stock's character.
///
/// These values are authored here, not derived from any third-party profile.
fn profile(stock: FilmStock) -> Profile {
    use FilmStock::*;
    match stock {
        Neutral => Profile::neutral(),

        // Kodak colour negative: warm, gentle saturation.
        KodakPortra160 => Profile {
            sat: 0.95,
            gain: [1.02, 1.00, 0.98],
            shadow: [0.02, 0.00, -0.01],
            highlight: [0.02, 0.01, -0.01],
            ..Profile::neutral()
        },
        KodakPortra160Vc => Profile {
            sat: 1.06,
            gain: [1.03, 1.00, 0.97],
            shadow: [0.02, 0.00, -0.01],
            ..Profile::neutral()
        },
        KodakPortra400 => Profile {
            sat: 0.97,
            gain: [1.02, 1.00, 0.97],
            shadow: [0.025, 0.005, -0.015],
            highlight: [0.02, 0.01, -0.01],
            ..Profile::neutral()
        },
        KodakPortra800 => Profile {
            sat: 0.98,
            gain: [1.03, 1.00, 0.96],
            shadow: [0.03, 0.005, -0.02],
            ..Profile::neutral()
        },
        FujifilmPro400h => Profile {
            sat: 0.92,
            gain: [0.99, 1.01, 1.00],
            shadow: [-0.01, 0.01, 0.02],
            highlight: [0.00, 0.01, 0.01],
            ..Profile::neutral()
        },
        FujiPro160c => Profile {
            sat: 1.00,
            gain: [1.00, 1.01, 0.99],
            shadow: [-0.005, 0.01, 0.015],
            ..Profile::neutral()
        },
        KodakEktar100 => Profile {
            sat: 1.15,
            gain: [1.03, 1.00, 0.97],
            shadow: [0.01, -0.005, 0.005],
            highlight: [0.01, 0.00, -0.005],
            ..Profile::neutral()
        },
        KodakGold100 => Profile {
            sat: 1.05,
            gain: [1.04, 1.00, 0.94],
            shadow: [0.03, 0.005, -0.02],
            ..Profile::neutral()
        },
        KodakGold200 => Profile {
            sat: 1.06,
            gain: [1.04, 1.00, 0.93],
            shadow: [0.035, 0.005, -0.025],
            ..Profile::neutral()
        },
        KodakUltramax400 => Profile {
            sat: 1.08,
            gain: [1.04, 1.00, 0.95],
            shadow: [0.03, 0.005, -0.02],
            ..Profile::neutral()
        },
        FujiSuperia400 => Profile {
            sat: 1.02,
            gain: [0.99, 1.02, 0.99],
            shadow: [-0.01, 0.02, 0.01],
            highlight: [-0.005, 0.01, 0.005],
            ..Profile::neutral()
        },
        FujiSuperia800 => Profile {
            sat: 1.03,
            gain: [0.99, 1.02, 0.98],
            shadow: [-0.01, 0.025, 0.015],
            ..Profile::neutral()
        },
        AgfaVista400 => Profile {
            sat: 1.05,
            gain: [1.02, 1.00, 0.98],
            shadow: [0.02, 0.00, 0.00],
            ..Profile::neutral()
        },

        // Black-and-white: a channel mixer and a paper tone.
        KodakTmax100 => Profile::bw([0.30, 0.59, 0.11], [1.00, 1.00, 1.00]),
        KodakTmax3200 => Profile::bw([0.32, 0.57, 0.11], [1.00, 1.00, 0.99]),
        IlfordHp5_400 => Profile::bw([0.34, 0.56, 0.10], [1.02, 1.00, 0.97]),
        IlfordFp4_125 => Profile::bw([0.30, 0.59, 0.11], [1.00, 1.00, 0.99]),
        KodakTrix400 => Profile::bw([0.36, 0.54, 0.10], [1.03, 1.00, 0.95]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Recipe;
    use crate::image::Image;
    use crate::params::{FilmParams, GrainParams};

    fn ctx_or_skip() -> Option<GpuContext> {
        match GpuContext::new_blocking() {
            Ok(ctx) => Some(ctx),
            Err(e) => {
                eprintln!("skipping GPU test: {e}");
                None
            }
        }
    }

    fn color_image(w: u32, h: u32) -> Image {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[
                    ((x * 255) / w.max(1)) as u8,
                    ((y * 255) / h.max(1)) as u8,
                    128,
                    255,
                ]);
            }
        }
        Image::from_rgba8(w, h, data).unwrap()
    }

    fn film_recipe(stock: FilmStock, intensity: f32) -> Recipe {
        Recipe {
            grain: GrainParams {
                intensity: 0.0,
                ..Default::default()
            },
            film: FilmParams {
                film_stock_enabled: true,
                film_stock: stock,
                film_stock_intensity: intensity,
            },
            ..Recipe::default()
        }
    }

    #[test]
    fn neutral_stock_is_identity() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = color_image(32, 32);
        let out =
            crate::render::render(&ctx, &img, &film_recipe(FilmStock::Neutral, 100.0)).unwrap();
        assert_eq!(out, img);
    }

    #[test]
    fn color_stock_changes_the_image() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = color_image(32, 32);
        let out = crate::render::render(&ctx, &img, &film_recipe(FilmStock::KodakPortra400, 100.0))
            .unwrap();
        assert_ne!(out, img);
    }

    #[test]
    fn bw_stock_produces_greys() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = color_image(32, 32);
        let out = crate::render::render(&ctx, &img, &film_recipe(FilmStock::IlfordHp5_400, 100.0))
            .unwrap();
        for px in out.data().chunks_exact(4) {
            // Tone may tint channels slightly; allow a small spread.
            let (r, g, b) = (px[0] as i32, px[1] as i32, px[2] as i32);
            assert!(
                (r - g).abs() <= 12 && (g - b).abs() <= 12,
                "bw output should be near-grey, got {r},{g},{b}"
            );
        }
    }

    #[test]
    fn every_stock_renders_and_only_neutral_is_identity() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = color_image(16, 16);
        for stock in crate::params::ALL_FILM_STOCKS {
            let out = crate::render::render(&ctx, &img, &film_recipe(stock, 100.0)).unwrap();
            if stock == FilmStock::Neutral {
                assert_eq!(out, img, "neutral must be a pass-through");
            } else {
                assert_ne!(out, img, "{stock} should change the image");
            }
        }
    }

    #[test]
    fn zero_intensity_is_identity() {
        let Some(ctx) = ctx_or_skip() else { return };
        let img = color_image(32, 32);
        let out =
            crate::render::render(&ctx, &img, &film_recipe(FilmStock::KodakEktar100, 0.0)).unwrap();
        assert_eq!(out, img);
    }
}
