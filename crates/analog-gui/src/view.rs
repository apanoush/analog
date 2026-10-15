//! The interactive photo view: cursor-anchored zoom and drag-to-pan.
//!
//! The preview is drawn by a [`canvas::Program`] so the image can be scaled and
//! translated (and clipped to the viewport) rather than simply fit to the
//! window. The view is described by a small, pure [`ViewTransform`]; the canvas
//! program only translates mouse events into new transforms, which the
//! application stores.

use iced::mouse::{self, Cursor, Interaction, ScrollDelta};
use iced::widget::canvas::{self, Frame, Geometry, Program};
use iced::widget::image;
use iced::{Event, Point, Rectangle, Renderer, Size, Theme, Vector};

use std::time::{Duration, Instant};

use crate::Message;

/// Maximum zoom relative to the fit-to-window scale.
const MAX_ZOOM: f32 = 8.0;
/// Zoom factor applied per unit of scroll.
const ZOOM_PER_SCROLL: f32 = 1.1;
/// Maximum delay between the two clicks of a double-click.
const DOUBLE_CLICK_DELAY: Duration = Duration::from_millis(400);
/// Maximum cursor movement between the two clicks of a double-click, in pixels.
const DOUBLE_CLICK_SLOP: f32 = 6.0;

/// The zoom and pan applied to the photo, relative to a fit-to-window view.
///
/// `zoom` is a multiplier on the fit-to-window scale (1.0 is fit, up to
/// [`MAX_ZOOM`]). `pan` is the offset of the image centre from the viewport
/// centre, in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewTransform {
    pub zoom: f32,
    pub pan: Vector,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self::fit()
    }
}

impl ViewTransform {
    /// A fit-to-window view: the whole image, centred.
    pub fn fit() -> Self {
        Self {
            zoom: 1.0,
            pan: Vector::ZERO,
        }
    }

    /// Whether the image is zoomed in beyond the fit-to-window view.
    pub fn is_zoomed(&self) -> bool {
        self.zoom > 1.0
    }

    /// The scale that makes `image` fit inside `viewport`.
    fn fit_scale(viewport: Size, image: Size) -> f32 {
        if image.width <= 0.0 || image.height <= 0.0 {
            return 1.0;
        }
        (viewport.width / image.width).min(viewport.height / image.height)
    }

    /// The drawn size of the image for this transform.
    fn scaled_size(&self, viewport: Size, image: Size) -> Size {
        scaled(image, Self::fit_scale(viewport, image) * self.zoom)
    }

    /// The rectangle the image is drawn into, in viewport-local coordinates.
    pub fn image_rect(&self, viewport: Size, image: Size) -> Rectangle {
        let size = self.scaled_size(viewport, image);
        let top_left = Point::new(
            (viewport.width - size.width) / 2.0 + self.pan.x,
            (viewport.height - size.height) / 2.0 + self.pan.y,
        );
        Rectangle::new(top_left, size)
    }

    /// Apply a multiplicative zoom step, keeping the image point under
    /// `cursor` fixed (cursor-anchored zoom).
    pub fn zoom_at(&self, factor: f32, cursor: Point, viewport: Size, image: Size) -> Self {
        let old_scale = Self::fit_scale(viewport, image) * self.zoom;
        let old = self.image_rect(viewport, image);
        // The image-space point (in image pixels) currently under the cursor.
        let anchor = Point::new(
            (cursor.x - old.x) / old_scale,
            (cursor.y - old.y) / old_scale,
        );

        let zoom = (self.zoom * factor).clamp(1.0, MAX_ZOOM);
        let new_scale = Self::fit_scale(viewport, image) * zoom;
        let size = scaled(image, new_scale);
        // Pan that maps `anchor` back under the cursor.
        let origin = Point::new(
            cursor.x - anchor.x * new_scale,
            cursor.y - anchor.y * new_scale,
        );
        let pan = Vector::new(
            origin.x - (viewport.width - size.width) / 2.0,
            origin.y - (viewport.height - size.height) / 2.0,
        );

        Self {
            zoom,
            pan: clamp_pan(pan, size, viewport),
        }
    }

    /// Translate the image by `delta` screen pixels.
    pub fn pan_by(&self, delta: Vector, viewport: Size, image: Size) -> Self {
        let size = self.scaled_size(viewport, image);
        Self {
            zoom: self.zoom,
            pan: clamp_pan(self.pan + delta, size, viewport),
        }
    }
}

/// The image size at the given scale.
fn scaled(image: Size, scale: f32) -> Size {
    Size::new(image.width * scale, image.height * scale)
}

/// Clamp `pan` so the image never leaves a gap inside the viewport. When the
/// scaled image is smaller than the viewport along an axis, that axis is
/// centred.
fn clamp_pan(pan: Vector, scaled: Size, viewport: Size) -> Vector {
    Vector::new(
        clamp_axis(pan.x, scaled.width, viewport.width),
        clamp_axis(pan.y, scaled.height, viewport.height),
    )
}

fn clamp_axis(value: f32, scaled: f32, viewport: f32) -> f32 {
    if scaled <= viewport {
        0.0
    } else {
        let half = (scaled - viewport) / 2.0;
        value.clamp(-half, half)
    }
}

/// The zoom factor for a scroll event. Scrolling up zooms in.
fn scroll_factor(delta: ScrollDelta) -> f32 {
    let lines = match delta {
        ScrollDelta::Lines { y, .. } => y,
        // A rough conversion so a pixel-based wheel steps similarly.
        ScrollDelta::Pixels { y, .. } => y / 50.0,
    };
    ZOOM_PER_SCROLL.powf(lines)
}

/// The interactive photo: draws the current handle with zoom/pan and turns
/// mouse events into view changes.
pub struct PhotoView {
    /// The handle shown (the render, or the original while comparing).
    pub handle: image::Handle,
    /// The natural size of the shown image, in pixels.
    pub image: Size,
    /// The current view transform.
    pub transform: ViewTransform,
}

/// Persistent canvas state: an in-progress drag and the last click.
#[derive(Default)]
pub struct PhotoState {
    drag: Option<Drag>,
    last_click: Option<(Point, Instant)>,
}

/// An in-progress pan.
struct Drag {
    /// Cursor position at the start of the drag, viewport-local.
    start: Point,
    /// The transform at the start of the drag.
    transform: ViewTransform,
}

impl Program<Message> for PhotoView {
    type State = PhotoState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<canvas::Action<Message>> {
        let viewport = bounds.size();

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let position = cursor.position_in(bounds)?;
                let factor = scroll_factor(*delta);
                let transform = self
                    .transform
                    .zoom_at(factor, position, viewport, self.image);
                Some(canvas::Action::publish(Message::ViewChanged(transform)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                let now = Instant::now();
                let is_double = state.last_click.is_some_and(|(last, at)| {
                    now.duration_since(at) < DOUBLE_CLICK_DELAY
                        && (position.x - last.x).hypot(position.y - last.y) < DOUBLE_CLICK_SLOP
                });
                state.last_click = Some((position, now));

                if is_double {
                    // Double-click resets to fit; don't start a drag.
                    state.drag = None;
                    return Some(canvas::Action::publish(Message::ViewReset).and_capture());
                }

                state.drag = Some(Drag {
                    start: position,
                    transform: self.transform,
                });
                Some(canvas::Action::capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let drag = state.drag.as_ref()?;
                let position = cursor.position_in(bounds)?;
                let delta = Vector::new(position.x - drag.start.x, position.y - drag.start.y);
                let transform = drag.transform.pan_by(delta, viewport, self.image);
                Some(canvas::Action::publish(Message::ViewChanged(transform)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.drag = None;
                Some(canvas::Action::capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let rect = self.transform.image_rect(bounds.size(), self.image);
        frame.draw_image(rect, &self.handle);
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        _cursor: Cursor,
    ) -> Interaction {
        if state.drag.is_some() {
            Interaction::Grabbing
        } else if self.transform.is_zoomed() {
            Interaction::Grab
        } else {
            Interaction::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: Size = Size::new(800.0, 600.0);
    const IMAGE: Size = Size::new(1000.0, 500.0);

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn fit_scale_uses_the_tight_axis() {
        assert!(close(ViewTransform::fit_scale(VIEWPORT, IMAGE), 0.8));
    }

    #[test]
    fn fit_is_centred() {
        let rect = ViewTransform::fit().image_rect(VIEWPORT, IMAGE);
        assert!(close(rect.x, 0.0));
        assert!(close(rect.y, 100.0));
        assert!(close(rect.width, 800.0));
        assert!(close(rect.height, 400.0));
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor_fixed() {
        let cursor = Point::new(200.0, 300.0);
        let zoomed = ViewTransform::fit().zoom_at(2.0, cursor, VIEWPORT, IMAGE);

        // The image point at the cursor before the zoom.
        let before = ViewTransform::fit().image_rect(VIEWPORT, IMAGE);
        let anchor = Point::new((cursor.x - before.x) / 0.8, (cursor.y - before.y) / 0.8);

        // After the zoom, cursor == rect.top_left + anchor * new_scale.
        let after = zoomed.image_rect(VIEWPORT, IMAGE);
        let scale = ViewTransform::fit_scale(VIEWPORT, IMAGE) * zoomed.zoom;
        assert!(close(after.x + anchor.x * scale, cursor.x));
        assert!(close(after.y + anchor.y * scale, cursor.y));
    }

    #[test]
    fn zoom_is_clamped() {
        let zoomed = ViewTransform::fit().zoom_at(100.0, Point::ORIGIN, VIEWPORT, IMAGE);
        assert!(close(zoomed.zoom, MAX_ZOOM));

        let out = ViewTransform::fit().zoom_at(0.01, Point::ORIGIN, VIEWPORT, IMAGE);
        assert!(close(out.zoom, 1.0));
    }

    #[test]
    fn pan_is_clamped_and_centred_when_smaller() {
        // Scaled 1600x800 in an 800x600 viewport: half-ranges are 400 and 100.
        let clamped = clamp_pan(
            Vector::new(1000.0, 1000.0),
            Size::new(1600.0, 800.0),
            VIEWPORT,
        );
        assert!(close(clamped.x, 400.0));
        assert!(close(clamped.y, 100.0));

        // Smaller than the viewport: forced to centre.
        let centred = clamp_pan(Vector::new(50.0, 50.0), Size::new(400.0, 200.0), VIEWPORT);
        assert_eq!(centred, Vector::ZERO);
    }

    #[test]
    fn panning_at_fit_stays_centred() {
        let panned = ViewTransform::fit().pan_by(Vector::new(50.0, 50.0), VIEWPORT, IMAGE);
        assert_eq!(panned.pan, Vector::ZERO);
    }
}
