//! `analog` command line interface.
//!
//! Commands: `info` (inspect an image), `gpu-info` (report the GPU adapter) and
//! `render` (decode, apply a recipe on the GPU, encode). All four effect blocks
//! are implemented: Film Color Shift, Grain, Halation and Print Simulation.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use analog_core::{DepthModel, FilmStock, GpuContext, Image, PaperType, Recipe, Renderer};

/// A local-first film grain editor.
#[derive(Debug, Parser)]
#[command(name = "analog", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print basic information about an image.
    Info {
        /// Path to the input image.
        input: PathBuf,
    },
    /// Print the selected GPU adapter and its relevant limits.
    GpuInfo,
    /// Convert a recipe between formats (.json, .analog, .xmp).
    Preset {
        /// Input recipe path.
        input: PathBuf,
        /// Output recipe path; format inferred from the extension.
        output: PathBuf,
    },
    /// Decode an image, apply a recipe and encode the result.
    Render {
        /// Path to the input image.
        input: PathBuf,
        /// Path to the output image; format is inferred from the extension.
        output: PathBuf,
        /// Optional recipe as JSON. Defaults to a neutral (pass-through) recipe.
        #[arg(long)]
        recipe: Option<PathBuf>,
        /// Grain intensity (0-100).
        #[arg(long, value_name = "0-100")]
        grain: Option<f32>,
        /// Halation intensity (0-100).
        #[arg(long, value_name = "0-100")]
        halation: Option<f32>,
        /// Print simulation intensity (0-100).
        #[arg(long, value_name = "0-100")]
        print: Option<f32>,
        /// Print simulation paper type.
        #[arg(long, value_enum)]
        paper: Option<PaperArg>,
        /// Film stock id (e.g. kodak_portra_400, ilford_hp5_400).
        #[arg(long, value_name = "ID")]
        film: Option<String>,
        /// Film color shift intensity (0-100); defaults to 100 with `--film`.
        #[arg(long, value_name = "0-100")]
        film_intensity: Option<f32>,
        /// Grain seed for deterministic grain.
        #[arg(long)]
        seed: Option<u64>,
        /// Depth-based Focus Control (0-100). Downloads the depth model on first use.
        #[arg(long, value_name = "0-100")]
        focus: Option<f32>,
    },
}

/// Paper type as a command line value.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum PaperArg {
    Matte,
    Neutral,
    Glossy,
}

impl From<PaperArg> for PaperType {
    fn from(value: PaperArg) -> Self {
        match value {
            PaperArg::Matte => PaperType::Matte,
            PaperArg::Neutral => PaperType::Neutral,
            PaperArg::Glossy => PaperType::Glossy,
        }
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    match cli.command {
        Command::Info { input } => info(&input),
        Command::GpuInfo => gpu_info(),
        Command::Preset { input, output } => preset(&input, &output),
        Command::Render {
            input,
            output,
            recipe,
            grain,
            halation,
            print,
            paper,
            film,
            film_intensity,
            seed,
            focus,
        } => {
            let mut recipe = match recipe {
                Some(path) => load_recipe(&path)?,
                None => Recipe::neutral(),
            };
            if let Some(g) = grain {
                recipe.grain.intensity = g;
            }
            if let Some(h) = halation {
                recipe.halation.halation_intensity = h;
            }
            if let Some(p) = print {
                recipe.print.print_simulation = p;
            }
            if let Some(p) = paper {
                recipe.print.print_sim_paper_type = p.into();
            }
            if let Some(id) = film {
                let stock = FilmStock::from_id(&id)
                    .with_context(|| format!("unknown film stock id '{id}'"))?;
                recipe.film.film_stock = stock;
                recipe.film.film_stock_enabled = stock != FilmStock::Neutral;
                recipe.film.film_stock_intensity = film_intensity.unwrap_or(100.0);
            } else if let Some(i) = film_intensity {
                recipe.film.film_stock_intensity = i;
            }
            if let Some(s) = seed {
                recipe.grain_seed = s;
            }
            if let Some(f) = focus {
                recipe.grain.depth_of_field_intensity = f;
            }
            render(&input, &output, recipe.clamped())
        }
    }
}

fn gpu_info() -> Result<()> {
    let ctx = GpuContext::new_blocking().context("creating GPU context")?;
    let info = &ctx.adapter_info;
    let limits = ctx.device.limits();
    println!("adapter:      {}", info.name);
    println!("backend:      {:?}", info.backend);
    println!("device type:  {:?}", info.device_type);
    println!("driver:       {} {}", info.driver, info.driver_info);
    println!(
        "max texture 2D:              {}",
        limits.max_texture_dimension_2d
    );
    println!(
        "max storage buffer binding:  {} bytes",
        limits.max_storage_buffer_binding_size
    );
    println!(
        "max buffer size:             {} bytes",
        limits.max_buffer_size
    );
    Ok(())
}

fn info(input: &Path) -> Result<()> {
    let img = Image::decode(input).with_context(|| format!("decoding {}", input.display()))?;
    println!(
        "{}: {}x{} {} ({} bytes)",
        input.display(),
        img.width(),
        img.height(),
        match img.format() {
            analog_core::PixelFormat::Rgba8 => "8-bit RGBA",
            analog_core::PixelFormat::Rgba16 => "16-bit RGBA",
        },
        img.data().len()
    );
    Ok(())
}

fn render(input: &Path, output: &Path, recipe: Recipe) -> Result<()> {
    let img = Image::decode(input).with_context(|| format!("decoding {}", input.display()))?;
    let ctx = GpuContext::new_blocking().context("creating GPU context")?;

    let depth = if recipe.needs_depth() {
        let mut model = DepthModel::load_or_download().context("loading depth model")?;
        log::info!("estimating depth for {}x{}", img.width(), img.height());
        Some(model.predict(&img).context("estimating depth")?)
    } else {
        None
    };

    let renderer = Renderer::new();
    let out = renderer
        .render(&ctx, &img, &recipe, depth.as_ref())
        .context("rendering")?;
    out.encode(output)
        .with_context(|| format!("encoding {}", output.display()))?;
    log::info!(
        "wrote {} ({}x{})",
        output.display(),
        out.width(),
        out.height()
    );
    Ok(())
}

fn load_recipe(path: &Path) -> Result<Recipe> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading recipe {}", path.display()))?;
    let is_json = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
        || text.trim_start().starts_with('{');
    let recipe = if is_json {
        serde_json::from_str(&text).with_context(|| format!("parsing recipe {}", path.display()))?
    } else {
        analog_core::from_recipe_xmp(&text)
            .with_context(|| format!("parsing recipe {}", path.display()))?
    };
    Ok(recipe.clamped())
}

fn write_recipe(path: &Path, recipe: &Recipe) -> Result<()> {
    let is_json = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    let text = if is_json {
        serde_json::to_string_pretty(recipe).context("serialising recipe")?
    } else {
        analog_core::to_recipe_xmp(recipe)
    };
    std::fs::write(path, text).with_context(|| format!("writing recipe {}", path.display()))?;
    Ok(())
}

fn preset(input: &Path, output: &Path) -> Result<()> {
    let recipe = load_recipe(input)?;
    write_recipe(output, &recipe)?;
    log::info!("wrote {}", output.display());
    Ok(())
}
