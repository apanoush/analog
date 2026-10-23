#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! `analog-gui`: the interactive editor.
//!
//! A single-window `iced` app: open a photo, adjust the four effect blocks and
//! see a live preview, then export at full resolution.
//!
//! Preview strategy: a coarse proxy is rendered immediately while the user drags
//! a control, and a full-resolution preview is rendered after a short debounce.
//! Renders run synchronously on the GPU-resident pipeline (one upload, one
//! readback), which is fast enough to feel interactive.
//!
//! Each block shows its common controls, with a per-section "Advanced" checkbox
//! revealing the rest.

mod view;

use std::ops::RangeInclusive;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use iced::gradient::Linear;
use iced::widget::{
    button, canvas, checkbox, column, container, image, mouse_area, pick_list, radio, row,
    scrollable, slider, space, text, tooltip,
};
use iced::{Background, Color, Element, Gradient, Length, Radians, Size, Theme};
use iced::{Subscription, Task};

use analog_core::{
    ALL_FILM_STOCKS, DepthMap, DepthModel, FilmStock, GpuContext, GrainType, Image, PaperType,
    Recipe, Renderer,
};

/// Longest side of the full preview proxy.
const PREVIEW_MAX: u32 = 900;
/// Longest side of the coarse proxy used while dragging.
const COARSE_MAX: u32 = 320;
/// Delay before the full preview renders after the last change.
const DEBOUNCE: Duration = Duration::from_millis(60);

/// The depth model, shared with the background estimation task.
type SharedDepthModel = Arc<Mutex<DepthModel>>;

/// An output image format the editor can export to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportFormat {
    Png,
    Jpeg,
    Tiff,
    WebP,
}

impl ExportFormat {
    /// Selectable formats, in menu order.
    const ALL: [ExportFormat; 4] = [Self::Png, Self::Jpeg, Self::Tiff, Self::WebP];

    /// Lower-case file extension for this format.
    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Tiff => "tiff",
            Self::WebP => "webp",
        }
    }
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Tiff => "TIFF",
            Self::WebP => "WebP",
        })
    }
}

fn main() -> iced::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    iced::application(App::new, update, view)
        .theme(Theme::Dark)
        .subscription(subscription)
        .run()
}

/// Application state.
struct App {
    ctx: GpuContext,
    renderer: Renderer,
    /// The full-resolution source image, used for export.
    source: Option<Image>,
    /// The full preview proxy.
    preview_src: Option<Image>,
    /// The coarse proxy used while dragging.
    coarse_src: Option<Image>,
    /// The rendered preview, ready for display.
    preview: Option<image::Handle>,
    /// The un-effected source proxy, shown while comparing.
    original_preview: Option<image::Handle>,
    /// Zoom and pan of the preview.
    transform: view::ViewTransform,
    /// Natural size of the preview proxy, in pixels.
    img_size: Size,
    /// True while the user is holding the compare control.
    show_original: bool,
    /// Depth model, loaded lazily the first time a depth control is used.
    depth_model: Option<SharedDepthModel>,
    /// Depth map for the preview proxy.
    preview_depth: Option<DepthMap>,
    /// True while a background depth estimate is in flight.
    depth_pending: bool,
    recipe: Recipe,
    /// Persistent line: the loaded image description.
    info: String,
    /// Transient line: progress or error, empty when idle.
    status: String,
    /// Set when a change is pending a full preview render.
    dirty: bool,
    /// Per-section "Advanced" disclosure state.
    advanced_grain: bool,
    advanced_halation: bool,
    advanced_print: bool,
}

impl App {
    fn new() -> Self {
        let ctx = GpuContext::new_blocking()
            .expect("analog requires a GPU; run `analog gpu-info` to diagnose");
        Self {
            ctx,
            renderer: Renderer::new(),
            source: None,
            preview_src: None,
            coarse_src: None,
            preview: None,
            original_preview: None,
            transform: view::ViewTransform::fit(),
            img_size: Size::ZERO,
            show_original: false,
            depth_model: None,
            preview_depth: None,
            depth_pending: false,
            recipe: Recipe::default(),
            info: "Open an image to begin.".into(),
            status: String::new(),
            dirty: false,
            advanced_grain: false,
            advanced_halation: false,
            advanced_print: false,
        }
    }

    fn load(&mut self, path: &Path) {
        match Image::decode(path) {
            Ok(img) => {
                self.info = format!("{} ({}x{})", path.display(), img.width(), img.height());
                self.status.clear();
                let proxy = img.thumbnail(PREVIEW_MAX);
                self.img_size = Size::new(proxy.width() as f32, proxy.height() as f32);
                self.transform = view::ViewTransform::fit();
                self.original_preview = Some(handle_from(&proxy));
                self.preview_src = Some(proxy);
                self.coarse_src = Some(img.thumbnail(COARSE_MAX));
                self.source = Some(img);
                self.preview_depth = None;
                self.depth_pending = false;
                self.show_original = false;
                self.render_preview(false);
            }
            Err(e) => self.status = format!("load error: {e}"),
        }
    }

    /// Request a background depth estimate if the recipe needs one and none is
    /// available. Returns a task that performs the model load and inference off
    /// the UI thread.
    fn maybe_request_depth(&mut self) -> Task<Message> {
        if !self.recipe.needs_depth() {
            self.preview_depth = None;
            return Task::none();
        }
        if self.preview_depth.is_some() || self.depth_pending {
            return Task::none();
        }
        let Some(src) = self.preview_src.clone() else {
            return Task::none();
        };
        self.status = "estimating depth...".into();
        self.depth_pending = true;
        let existing = self.depth_model.clone();
        Task::perform(
            async move {
                let model = match existing {
                    Some(model) => model,
                    None => Arc::new(Mutex::new(
                        DepthModel::load_or_download().map_err(|e| e.to_string())?,
                    )),
                };
                let depth = {
                    let mut guard = model.lock().expect("depth model mutex");
                    guard.predict(&src).map_err(|e| e.to_string())?
                };
                Ok((model, depth))
            },
            Message::DepthReady,
        )
    }

    /// Estimate depth synchronously, for export.
    fn depth_for(&mut self, src: &Image) -> Option<DepthMap> {
        if self.depth_model.is_none() {
            match DepthModel::load_or_download() {
                Ok(model) => self.depth_model = Some(Arc::new(Mutex::new(model))),
                Err(e) => {
                    self.status = format!("depth model error: {e}");
                    return None;
                }
            }
        }
        let model = self.depth_model.as_ref().expect("loaded above");
        match model.lock().expect("depth model mutex").predict(src) {
            Ok(depth) => Some(depth),
            Err(e) => {
                self.status = format!("depth error: {e}");
                None
            }
        }
    }

    /// Render the preview, either from the coarse proxy or the full proxy.
    fn render_preview(&mut self, coarse: bool) {
        let src = if coarse {
            self.coarse_src.take()
        } else {
            self.preview_src.take()
        };
        if let Some(src) = &src {
            // Depth only applies to the full preview; the coarse pass ignores it.
            let depth = if coarse {
                None
            } else {
                self.preview_depth.as_ref()
            };
            // Grain is normalised to the source size so the proxy preview
            // matches the full-resolution export at fit.
            let reference = self
                .source
                .as_ref()
                .map(|image| image.width().max(image.height()));
            match self.renderer.render_with_reference(
                &self.ctx,
                src,
                &self.recipe,
                depth,
                reference,
            ) {
                Ok(img) => {
                    self.preview = Some(handle_from(&img));
                }
                Err(e) => self.status = format!("render error: {e}"),
            }
        }
        if coarse {
            self.coarse_src = src;
        } else {
            self.preview_src = src;
        }
    }

    /// Render the coarse proxy now, mark the full preview as pending, and
    /// request a depth estimate if one is needed.
    fn interactive_render(&mut self) -> Task<Message> {
        self.render_preview(true);
        self.dirty = true;
        self.maybe_request_depth()
    }

    /// Render at full resolution and save in `format`.
    fn export(&mut self, format: ExportFormat) {
        let source = self.source.take();
        let Some(src) = &source else {
            self.status = "no image loaded".into();
            return;
        };
        let default_name = format!("grained.{}", format.extension());
        if let Some(mut path) = rfd::FileDialog::new()
            .set_file_name(default_name)
            .save_file()
        {
            if path.extension().is_none() {
                path.set_extension(format.extension());
            }
            let depth = if self.recipe.needs_depth() {
                self.status = "estimating depth...".into();
                self.depth_for(src)
            } else {
                None
            };
            match self
                .renderer
                .render(&self.ctx, src, &self.recipe, depth.as_ref())
            {
                Ok(img) => match img.encode(&path) {
                    Ok(()) => self.status = format!("exported {}", path.display()),
                    Err(e) => self.status = format!("export error: {e}"),
                },
                Err(e) => self.status = format!("render error: {e}"),
            }
        }
        self.source = source;
    }
}

/// UI messages.
#[derive(Clone)]
enum Message {
    OpenPressed,
    /// Export in the chosen format: the merged Export control's selection.
    ExportWith(ExportFormat),
    Reseed,
    // Grain
    GrainIntensity(f32),
    GrainAmount(f32),
    GrainSoftness(f32),
    GrainColor(f32),
    GrainColorShift(f32),
    GrainFusion(f32),
    FineGrain(f32),
    LumHighlights(f32),
    LumMids(f32),
    LumShadows(f32),
    GrainType(GrainType),
    Focus(f32),
    // Halation
    HalationIntensity(f32),
    HalationRadius(f32),
    HalationThreshold(f32),
    HalationColor(f32),
    HalationColorShift(f32),
    HalationColorTone(f32),
    HalationBackgroundFocus(f32),
    HalationBackgroundGain(f32),
    LightSpill(f32),
    // Print simulation
    PrintSimulation(f32),
    PaperType(PaperType),
    PaperIntensity(f32),
    HighlightRolloff(f32),
    PaperTint(f32),
    // Film color shift
    FilmIntensity(f32),
    FilmStock(FilmStock),
    // Disclosure
    ToggleGrainAdvanced(bool),
    ToggleHalationAdvanced(bool),
    TogglePrintAdvanced(bool),
    /// Press the compare control: show the un-effected original.
    CompareStart,
    /// Release the compare control: return to the effect.
    CompareEnd,
    /// The preview zoom or pan changed.
    ViewChanged(view::ViewTransform),
    /// Reset the preview to fit-to-window.
    ViewReset,
    /// A keyboard event (hold a key to compare).
    Key(iced::keyboard::Event),
    /// Result of a background depth estimate: the shared model and the map.
    DepthReady(Result<(SharedDepthModel, DepthMap), String>),
    /// Periodic tick that renders the full preview when a change is pending.
    Tick,
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::OpenPressed => {
            let mut exts = vec!["jpg", "jpeg", "png", "tif", "tiff", "psd", "webp"];
            if cfg!(feature = "raw") {
                exts.extend_from_slice(analog_core::RAW_EXTENSIONS);
            }
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Images", &exts)
                .pick_file()
            {
                app.load(&path);
                return app.maybe_request_depth();
            }
            Task::none()
        }
        Message::ExportWith(f) => {
            app.export(f);
            Task::none()
        }
        Message::Reseed => {
            app.recipe.grain_seed = random_seed();
            app.interactive_render()
        }
        Message::GrainIntensity(v) => {
            app.recipe.grain.intensity = v;
            app.interactive_render()
        }
        Message::GrainAmount(v) => {
            app.recipe.grain.grain_amount = v;
            app.interactive_render()
        }
        Message::GrainSoftness(v) => {
            app.recipe.grain.roughness = v;
            app.interactive_render()
        }
        Message::GrainColor(v) => {
            app.recipe.grain.grain_color = v;
            app.interactive_render()
        }
        Message::GrainColorShift(v) => {
            app.recipe.grain.color_shift = v;
            app.interactive_render()
        }
        Message::GrainFusion(v) => {
            app.recipe.grain.dissolution_amount = v;
            app.interactive_render()
        }
        Message::FineGrain(v) => {
            app.recipe.grain.fine_grain_intensity = v;
            app.interactive_render()
        }
        Message::LumHighlights(v) => {
            app.recipe.grain.luminance_highlights = v;
            app.interactive_render()
        }
        Message::LumMids(v) => {
            app.recipe.grain.luminance_mids = v;
            app.interactive_render()
        }
        Message::LumShadows(v) => {
            app.recipe.grain.luminance_shadows = v;
            app.interactive_render()
        }
        Message::GrainType(t) => {
            app.recipe.grain.grain_type = t;
            app.interactive_render()
        }
        Message::Focus(v) => {
            app.recipe.grain.depth_of_field_intensity = v;
            app.interactive_render()
        }
        Message::HalationIntensity(v) => {
            app.recipe.halation.halation_intensity = v;
            app.interactive_render()
        }
        Message::HalationRadius(v) => {
            app.recipe.halation.halation_radius = v;
            app.interactive_render()
        }
        Message::HalationThreshold(v) => {
            app.recipe.halation.halation_threshold = v;
            app.interactive_render()
        }
        Message::HalationColor(v) => {
            app.recipe.halation.halation_color_intensity = v;
            app.interactive_render()
        }
        Message::HalationColorShift(v) => {
            app.recipe.halation.halation_color_shift = v;
            app.interactive_render()
        }
        Message::HalationColorTone(v) => {
            app.recipe.halation.halation_color_tone = v;
            app.interactive_render()
        }
        Message::HalationBackgroundFocus(v) => {
            app.recipe.halation.halation_background_focus = v;
            app.interactive_render()
        }
        Message::HalationBackgroundGain(v) => {
            app.recipe.halation.halation_background_gain = v;
            app.interactive_render()
        }
        Message::LightSpill(v) => {
            app.recipe.halation.light_spill_intensity = v;
            app.interactive_render()
        }
        Message::PrintSimulation(v) => {
            app.recipe.print.print_simulation = v;
            app.interactive_render()
        }
        Message::PaperType(p) => {
            app.recipe.print.print_sim_paper_type = p;
            app.interactive_render()
        }
        Message::PaperIntensity(v) => {
            app.recipe.print.print_sim_paper_intensity = v;
            app.interactive_render()
        }
        Message::HighlightRolloff(v) => {
            app.recipe.print.print_sim_highlight_rolloff = v;
            app.interactive_render()
        }
        Message::PaperTint(v) => {
            app.recipe.print.print_sim_paper_tint = v;
            app.interactive_render()
        }
        Message::FilmIntensity(v) => {
            app.recipe.film.film_stock_intensity = v;
            app.interactive_render()
        }
        Message::FilmStock(s) => {
            app.recipe.film.film_stock = s;
            app.recipe.film.film_stock_enabled = s != FilmStock::Neutral;
            if s != FilmStock::Neutral && app.recipe.film.film_stock_intensity == 0.0 {
                app.recipe.film.film_stock_intensity = 100.0;
            }
            app.interactive_render()
        }
        Message::ToggleGrainAdvanced(v) => {
            app.advanced_grain = v;
            Task::none()
        }
        Message::ToggleHalationAdvanced(v) => {
            app.advanced_halation = v;
            Task::none()
        }
        Message::TogglePrintAdvanced(v) => {
            app.advanced_print = v;
            Task::none()
        }
        Message::DepthReady(result) => {
            app.depth_pending = false;
            match result {
                Ok((model, depth)) => {
                    app.depth_model = Some(model);
                    if app.recipe.needs_depth() {
                        app.preview_depth = Some(depth);
                        app.status.clear();
                        app.render_preview(false);
                    } else {
                        // The control was turned off while we were working.
                        app.preview_depth = None;
                        app.status.clear();
                    }
                }
                Err(e) => app.status = format!("depth error: {e}"),
            }
            Task::none()
        }
        Message::Tick => {
            if app.dirty {
                app.dirty = false;
                app.render_preview(false);
            }
            Task::none()
        }
        Message::CompareStart => {
            if app.original_preview.is_some() {
                app.show_original = true;
            }
            Task::none()
        }
        Message::CompareEnd => {
            app.show_original = false;
            Task::none()
        }
        Message::ViewChanged(transform) => {
            app.transform = transform;
            Task::none()
        }
        Message::ViewReset => {
            app.transform = view::ViewTransform::fit();
            Task::none()
        }
        Message::Key(event) => {
            if app.original_preview.is_some() {
                match event {
                    iced::keyboard::Event::KeyPressed { key, .. } if is_compare_key(&key) => {
                        app.show_original = true;
                    }
                    iced::keyboard::Event::KeyReleased { key, .. } if is_compare_key(&key) => {
                        app.show_original = false;
                    }
                    _ => {}
                }
            }
            Task::none()
        }
    }
}

/// The periodic debounce tick plus the keyboard compare hold.
fn subscription(_app: &App) -> Subscription<Message> {
    Subscription::batch([
        iced::time::every(DEBOUNCE).map(|_| Message::Tick),
        iced::keyboard::listen().map(Message::Key),
    ])
}

/// Whether a key is the compare-hold key (backslash or `B`).
fn is_compare_key(key: &iced::keyboard::Key) -> bool {
    match key {
        iced::keyboard::Key::Character(c) => {
            let c = c.as_ref();
            c == "\\" || c.eq_ignore_ascii_case("b")
        }
        _ => false,
    }
}

/// Build an 8-bit image handle for display.
fn handle_from(img: &Image) -> image::Handle {
    let display = img.to_rgba8();
    image::Handle::from_rgba(display.width(), display.height(), display.data().to_vec())
}

fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn view(app: &App) -> Element<'_, Message> {
    let compare = compare_control(app.original_preview.is_some());
    // The transient status is only shown when it has something to say.
    let status: Element<'_, Message> = if app.status.is_empty() {
        space().into()
    } else {
        text(app.status.as_str()).size(12).into()
    };
    let controls = column![
        row![
            button("Open").on_press(Message::OpenPressed),
            pick_list(
                &ExportFormat::ALL[..],
                None::<ExportFormat>,
                Message::ExportWith,
            )
            .placeholder("Export…"),
            button("Fit").on_press(Message::ViewReset),
            compare,
        ]
        .spacing(8),
        text(app.info.as_str()).size(12),
        status,
        grain_section(app),
        halation_section(app),
        print_section(app),
        film_section(app),
    ]
    .spacing(10)
    .padding(12)
    .width(320);

    // While comparing, show the un-effected original proxy instead of the render.
    let shown = if app.show_original {
        app.original_preview.as_ref()
    } else {
        app.preview.as_ref()
    };
    let preview: Element<'_, Message> = match shown {
        Some(handle) => canvas(view::PhotoView {
            handle: handle.clone(),
            image: app.img_size,
            transform: app.transform,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
        None => text("No image loaded").into(),
    };

    row![
        scrollable(controls),
        container(preview)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    ]
    .spacing(12)
    .padding(12)
    .into()
}

/// The Grain block.
fn grain_section(app: &App) -> Element<'_, Message> {
    let mut col = column![
        section("Grain"),
        slider_row(
            "Intensity",
            0.0..=100.0,
            app.recipe.grain.intensity,
            Message::GrainIntensity
        ),
        slider_row(
            "Size (ISO)",
            0.0..=100.0,
            app.recipe.grain.grain_amount,
            Message::GrainAmount
        ),
        slider_row(
            "Softness",
            0.0..=100.0,
            app.recipe.grain.roughness,
            Message::GrainSoftness
        ),
        slider_row_gradient(
            "Color",
            0.0..=100.0,
            app.recipe.grain.grain_color,
            Message::GrainColor,
            &grain_color_ramp()
        ),
        slider_row(
            "Clumping",
            0.0..=100.0,
            app.recipe.grain.dissolution_amount,
            Message::GrainFusion
        ),
        slider_row(
            "Highlights",
            0.0..=100.0,
            app.recipe.grain.luminance_highlights,
            Message::LumHighlights
        ),
        slider_row(
            "Mids",
            0.0..=100.0,
            app.recipe.grain.luminance_mids,
            Message::LumMids
        ),
        slider_row(
            "Shadows",
            0.0..=100.0,
            app.recipe.grain.luminance_shadows,
            Message::LumShadows
        ),
        slider_row(
            "Focus Control",
            0.0..=100.0,
            app.recipe.grain.depth_of_field_intensity,
            Message::Focus
        ),
        row![
            radio(
                "Coarse",
                GrainType::Coarse,
                Some(app.recipe.grain.grain_type),
                Message::GrainType
            ),
            radio(
                "Smooth",
                GrainType::Smooth,
                Some(app.recipe.grain.grain_type),
                Message::GrainType
            ),
            radio(
                "Fine",
                GrainType::Fine,
                Some(app.recipe.grain.grain_type),
                Message::GrainType
            ),
        ]
        .spacing(8),
        checkbox(app.advanced_grain)
            .label("Advanced")
            .on_toggle(Message::ToggleGrainAdvanced),
    ]
    .spacing(10);
    if app.advanced_grain {
        col = col
            .push(slider_row_gradient(
                "Color Shift",
                0.0..=100.0,
                app.recipe.grain.color_shift,
                Message::GrainColorShift,
                &grain_shift_ramp(),
            ))
            .push(slider_row(
                "Fine Grain",
                0.0..=100.0,
                app.recipe.grain.fine_grain_intensity,
                Message::FineGrain,
            ));
    }
    col.push(button("Reseed grain").on_press(Message::Reseed))
        .into()
}

/// The Halation block.
fn halation_section(app: &App) -> Element<'_, Message> {
    // The "Color" swatch shows the actual tint at the current tone/shift.
    let tint = analog_core::tint_rgb(&app.recipe.halation);
    let tint_color = Color::from_rgb(
        tint[0].clamp(0.0, 1.0),
        tint[1].clamp(0.0, 1.0),
        tint[2].clamp(0.0, 1.0),
    );
    let color_ramp = vec![(0.0, Color::WHITE), (1.0, tint_color)];
    let mut col = column![
        section("Halation"),
        slider_row(
            "Intensity",
            0.0..=100.0,
            app.recipe.halation.halation_intensity,
            Message::HalationIntensity
        ),
        slider_row(
            "Radius",
            0.0..=100.0,
            app.recipe.halation.halation_radius,
            Message::HalationRadius
        ),
        slider_row_gradient(
            "Color",
            0.0..=100.0,
            app.recipe.halation.halation_color_intensity,
            Message::HalationColor,
            &color_ramp
        ),
        slider_row_gradient(
            "Color Tone",
            0.0..=100.0,
            app.recipe.halation.halation_color_tone,
            Message::HalationColorTone,
            &halation_ramp()
        ),
        slider_row(
            "Background Focus",
            0.0..=100.0,
            app.recipe.halation.halation_background_focus,
            Message::HalationBackgroundFocus
        ),
        checkbox(app.advanced_halation)
            .label("Advanced")
            .on_toggle(Message::ToggleHalationAdvanced),
    ]
    .spacing(10);
    if app.advanced_halation {
        col = col
            .push(slider_row(
                "Threshold",
                0.0..=255.0,
                app.recipe.halation.halation_threshold,
                Message::HalationThreshold,
            ))
            .push(slider_row_gradient(
                "Color Shift",
                0.0..=100.0,
                app.recipe.halation.halation_color_shift,
                Message::HalationColorShift,
                &halation_ramp(),
            ))
            .push(slider_row(
                "Background Gain",
                0.0..=100.0,
                app.recipe.halation.halation_background_gain,
                Message::HalationBackgroundGain,
            ))
            .push(slider_row(
                "Light Spill",
                0.0..=100.0,
                app.recipe.halation.light_spill_intensity,
                Message::LightSpill,
            ));
    }
    col.into()
}

/// The Print Simulation block.
fn print_section(app: &App) -> Element<'_, Message> {
    let mut col = column![
        section("Print Simulation"),
        slider_row(
            "Intensity",
            0.0..=100.0,
            app.recipe.print.print_simulation,
            Message::PrintSimulation
        ),
        row![
            radio(
                "Matte",
                PaperType::Matte,
                Some(app.recipe.print.print_sim_paper_type),
                Message::PaperType
            ),
            radio(
                "Neutral",
                PaperType::Neutral,
                Some(app.recipe.print.print_sim_paper_type),
                Message::PaperType
            ),
            radio(
                "Glossy",
                PaperType::Glossy,
                Some(app.recipe.print.print_sim_paper_type),
                Message::PaperType
            ),
        ]
        .spacing(8),
        checkbox(app.advanced_print)
            .label("Advanced")
            .on_toggle(Message::TogglePrintAdvanced),
    ]
    .spacing(10);
    if app.advanced_print {
        col = col
            .push(slider_row(
                "Paper Intensity",
                0.0..=100.0,
                app.recipe.print.print_sim_paper_intensity,
                Message::PaperIntensity,
            ))
            .push(slider_row(
                "Highlight Rolloff",
                0.0..=100.0,
                app.recipe.print.print_sim_highlight_rolloff,
                Message::HighlightRolloff,
            ))
            .push(slider_row_gradient(
                "Paper Tint",
                -50.0..=50.0,
                app.recipe.print.print_sim_paper_tint,
                Message::PaperTint,
                &paper_tint_ramp(),
            ));
    }
    col.into()
}

/// The Film Color Shift block.
fn film_section(app: &App) -> Element<'_, Message> {
    column![
        section("Film Color Shift"),
        pick_list(
            &ALL_FILM_STOCKS[..],
            Some(app.recipe.film.film_stock),
            Message::FilmStock,
        ),
        slider_row(
            "Intensity",
            0.0..=100.0,
            app.recipe.film.film_stock_intensity,
            Message::FilmIntensity
        ),
    ]
    .spacing(10)
    .into()
}

/// A small heading.
fn section(title: &str) -> Element<'_, Message> {
    text(title).size(15).into()
}

/// The hold-to-compare control: press to show the original, release to return.
///
/// A short label keeps the toolbar row within the control column; the full
/// explanation lives in a tooltip.
fn compare_control(enabled: bool) -> Element<'static, Message> {
    let label = container(text("Compare (B)").size(13)).padding([4, 8]);
    if enabled {
        tooltip(
            mouse_area(label)
                .on_press(Message::CompareStart)
                .on_release(Message::CompareEnd),
            container(text("Hold to compare with the original (B)").size(12))
                .padding(6)
                .style(container::rounded_box),
            tooltip::Position::Bottom,
        )
        .into()
    } else {
        label.into()
    }
}

/// A labelled slider over an arbitrary range.
fn slider_row(
    label: &str,
    range: RangeInclusive<f32>,
    value: f32,
    on_change: fn(f32) -> Message,
) -> Element<'_, Message> {
    slider_row_gradient(label, range, value, on_change, &[])
}

/// A labelled slider with an optional colour-ramp swatch above the track.
fn slider_row_gradient<'a>(
    label: &'a str,
    range: RangeInclusive<f32>,
    value: f32,
    on_change: fn(f32) -> Message,
    stops: &[(f32, Color)],
) -> Element<'a, Message> {
    let mut col = column![text(format!("{label}: {value:.0}")).size(12)];
    if !stops.is_empty() {
        col = col.push(gradient_swatch(stops));
    }
    col.push(slider(range, value, on_change)).spacing(2).into()
}

/// A horizontal colour-ramp swatch, indicating what a slider controls.
///
/// The ramp is a guide: the rendered result also depends on the image and the
/// other parameters.
fn gradient_swatch(stops: &[(f32, Color)]) -> Element<'static, Message> {
    let mut linear = Linear::new(Radians(std::f32::consts::FRAC_PI_2));
    for (offset, color) in stops {
        linear = linear.add_stop(*offset, *color);
    }
    container(text(""))
        .width(Length::Fill)
        .height(Length::Fixed(10.0))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Gradient(Gradient::Linear(linear))),
            border: iced::border::rounded(4.0),
            ..container::Style::default()
        })
        .into()
}

/// An 8-bit colour.
fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

/// The yellow-to-red ramp used by the halation tint.
fn halation_ramp() -> Vec<(f32, Color)> {
    vec![(0.0, rgb(255, 204, 90)), (1.0, rgb(255, 77, 30))]
}

/// Grey (monochrome) to a warm chroma, for grain colour.
fn grain_color_ramp() -> Vec<(f32, Color)> {
    vec![(0.0, rgb(128, 128, 128)), (1.0, rgb(205, 120, 60))]
}

/// A hue sweep, for the grain chromatic direction.
fn grain_shift_ramp() -> Vec<(f32, Color)> {
    vec![
        (0.0, rgb(70, 110, 190)),
        (0.25, rgb(70, 170, 170)),
        (0.5, rgb(150, 180, 90)),
        (0.75, rgb(210, 150, 70)),
        (1.0, rgb(200, 90, 70)),
    ]
}

/// Cool cyan-blue to warm cream, for the print paper tint.
fn paper_tint_ramp() -> Vec<(f32, Color)> {
    vec![
        (0.0, rgb(159, 188, 214)),
        (0.5, rgb(232, 228, 220)),
        (1.0, rgb(242, 226, 194)),
    ]
}

#[cfg(test)]
mod app_tests {
    use super::*;

    fn test_app() -> Option<App> {
        if GpuContext::new_blocking().is_err() {
            eprintln!("skipping GUI test: no GPU");
            return None;
        }
        Some(App::new())
    }

    fn temp_image() -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("analog-gui-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("in.png");
        let data: Vec<u8> = (0..32 * 24 * 4)
            .map(|i| if i % 4 == 3 { 255 } else { (i * 5) as u8 })
            .collect();
        Image::from_rgba8(32, 24, data)
            .unwrap()
            .encode(&path)
            .unwrap();
        (dir, path)
    }

    #[test]
    fn view_builds_with_and_without_advanced_sections() {
        let Some(mut app) = test_app() else { return };
        let _ = view(&app);
        app.advanced_grain = true;
        app.advanced_halation = true;
        app.advanced_print = true;
        let _ = view(&app);
    }

    #[test]
    fn slider_and_toggle_messages_update_the_recipe() {
        let Some(mut app) = test_app() else { return };
        let messages = [
            Message::GrainIntensity(61.0),
            Message::GrainAmount(22.0),
            Message::GrainSoftness(8.0),
            Message::GrainColor(12.0),
            Message::GrainColorShift(44.0),
            Message::GrainFusion(5.0),
            Message::FineGrain(7.0),
            Message::LumHighlights(20.0),
            Message::LumMids(33.0),
            Message::LumShadows(66.0),
            Message::GrainType(GrainType::Smooth),
            Message::Focus(11.0),
            Message::HalationIntensity(12.0),
            Message::HalationRadius(13.0),
            Message::HalationThreshold(170.0),
            Message::HalationColor(14.0),
            Message::HalationColorShift(15.0),
            Message::HalationColorTone(16.0),
            Message::HalationBackgroundFocus(17.0),
            Message::HalationBackgroundGain(18.0),
            Message::LightSpill(19.0),
            Message::PrintSimulation(21.0),
            Message::PaperType(PaperType::Matte),
            Message::PaperIntensity(23.0),
            Message::HighlightRolloff(24.0),
            Message::PaperTint(-5.0),
            Message::FilmIntensity(26.0),
            Message::FilmStock(FilmStock::KodakPortra400),
            Message::ToggleGrainAdvanced(true),
            Message::ToggleHalationAdvanced(true),
            Message::TogglePrintAdvanced(true),
            Message::CompareStart,
            Message::CompareEnd,
            Message::ViewReset,
            Message::ViewChanged(view::ViewTransform::fit()),
            Message::Tick,
        ];
        for message in messages {
            let _ = update(&mut app, message);
        }
        assert_eq!(app.recipe.grain.intensity, 61.0);
        assert_eq!(app.recipe.grain.grain_type, GrainType::Smooth);
        assert_eq!(app.recipe.halation.halation_threshold, 170.0);
        assert_eq!(app.recipe.print.print_sim_paper_type, PaperType::Matte);
        assert_eq!(app.recipe.film.film_stock, FilmStock::KodakPortra400);
        assert!(app.advanced_grain && app.advanced_halation && app.advanced_print);
    }

    #[test]
    fn load_builds_preview_and_depth_reports_update_status() {
        let Some(mut app) = test_app() else { return };
        let (dir, path) = temp_image();
        app.load(&path);
        assert!(app.source.is_some(), "source should be set");
        assert!(app.preview_src.is_some(), "preview proxy should be set");
        assert!(app.preview.is_some(), "preview render should succeed");
        assert_eq!(app.img_size.width, 32.0);

        let _ = update(&mut app, Message::DepthReady(Err("no model".into())));
        assert!(app.status.contains("no model"));
        assert!(!app.depth_pending);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn depth_request_is_ignored_without_a_depth_control() {
        let Some(mut app) = test_app() else { return };
        app.recipe.grain.depth_of_field_intensity = 0.0;
        let _ = app.maybe_request_depth();
        assert!(app.preview_depth.is_none());

        let (dir, path) = temp_image();
        app.load(&path);
        app.recipe.grain.depth_of_field_intensity = 20.0;
        let _ = app.maybe_request_depth();
        assert!(app.depth_pending);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
