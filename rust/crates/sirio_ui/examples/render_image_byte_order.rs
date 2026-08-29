//! #268: is `RenderImage`'s pixel data BGRA or RGBA? Settle it by looking.
//!
//! gpui contradicts itself -- `RenderImage`'s doc says BGRA, `repl`'s image
//! output swaps R and B before building one, and `livekit_client` does not and
//! says it does not know why that works. Reading more code cannot settle it.
//!
//! Each swatch below is built from bytes in a KNOWN order and labelled with
//! what it must look like under each interpretation. The rendered colour is
//! the answer.
//!
//! Run: cargo run -p sirio_ui --example render_image_byte_order

use std::sync::Arc;

use gpui::{
    App, AppContext, Bounds, Context, Corners, Render, RenderImage, Window, WindowBounds,
    WindowOptions, canvas, div, prelude::*, px, size,
};
use gpui_platform::application;
use image::{Frame, RgbaImage};

/// A solid swatch whose four bytes per pixel are exactly `bytes`, in that
/// order in memory. No interpretation is applied here -- that is the point.
fn swatch(bytes: [u8; 4]) -> Arc<RenderImage> {
    const SIDE: u32 = 64;
    let mut buffer = RgbaImage::new(SIDE, SIDE);
    for pixel in buffer.pixels_mut() {
        pixel.0 = bytes;
    }
    Arc::new(RenderImage::new(vec![Frame::new(buffer)]))
}

/// A gradient that is asymmetric in every channel, so a swap of any two is
/// visible rather than plausible.
fn gradient() -> Arc<RenderImage> {
    const W: u32 = 128;
    const H: u32 = 64;
    let mut buffer = RgbaImage::new(W, H);
    for (x, _y, pixel) in buffer.enumerate_pixels_mut() {
        let t = (x * 255 / (W - 1)) as u8;
        // byte0 ramps up, byte1 is fixed low, byte2 ramps down.
        pixel.0 = [t, 32, 255 - t, 255];
    }
    Arc::new(RenderImage::new(vec![Frame::new(buffer)]))
}

struct Probe {
    swatches: Vec<(&'static str, Arc<RenderImage>)>,
}

impl Render for Probe {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let swatches = self.swatches.clone();
        div()
            .size_full()
            .bg(gpui::white())
            .child(canvas(
                move |_, _, _| {},
                move |bounds, _, window, _| {
                    for (index, (_, image)) in swatches.iter().enumerate() {
                        let origin = gpui::point(
                            bounds.origin.x + px(20.0) + px(150.0 * index as f32),
                            bounds.origin.y + px(40.0),
                        );
                        let target = Bounds::new(origin, size(px(128.0), px(64.0)));
                        let _ = window.paint_image(
                            target,
                            target,
                            Corners::default(),
                            image.clone(),
                            0,
                            false,
                        );
                    }
                },
            ))
            .size_full()
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(700.0), px(200.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| Probe {
                    swatches: vec![
                        // bytes 255,0,0,255 -- red under RGBA, blue under BGRA
                        ("bytes R-first", swatch([255, 0, 0, 255])),
                        // bytes 0,0,255,255 -- blue under RGBA, red under BGRA
                        ("bytes B-first", swatch([0, 0, 255, 255])),
                        ("gradient", gradient()),
                    ],
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
