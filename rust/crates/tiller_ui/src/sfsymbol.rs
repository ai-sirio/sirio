//! macOS-only: rasterize SF Symbols (Apple's system icon API) into RGBA
//! bitmaps GPUI can paint.
//!
//! The Swift app renders its bar and tab icons with `Image(systemName:)`,
//! which resolves the real SF Symbol through AppKit — weight, scale and
//! fill variants included, all at the system's own quality. Nothing
//! downloadable imitates them exactly, so on macOS this crate resolves its
//! `Icon` enum against the same API: `NSImage(systemSymbolName:
//! accessibilityDescription:)`, rasterized offscreen at 2x (matching GPUI's
//! `SMOOTH_SVG_SCALE_FACTOR` for the SVG path), and handed to GPUI as a
//! premultiplied BGRA [`gpui::RenderImage`].
//!
//! Symbols are template images: their pixels are a black shape over an
//! alpha mask. The colour is applied here, at rasterization time, by
//! multiplying the tint into the RGB channels exactly as GPUI's SVG path
//! tints an alpha mask — so the result is a premultiplied image of the
//! glyph in the caller's theme colour, cached per (symbol, size, tint).
//!
//! Only the default-weight, regular-scale symbol is ever requested — the
//! reference app does the same (`.imageScale(.small)` at most) — so no
//! symbol configuration API is needed, just the bare `systemSymbolName`
//! constructor and a bitmap representation.
//!
//! This module is macOS-only by `#[cfg]`; every other target resolves the
//! same `Icon` enum against the embedded Phosphor SVGs instead, and the
//! rest of the crate never sees either mechanism.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use gpui::RenderImage;
use image::{Frame, ImageBuffer, Rgba as ImageRgba};
use smallvec::SmallVec;

/// The pixel scale used for rasterization, matching GPUI's own SVG renderer
/// (`SMOOTH_SVG_SCALE_FACTOR`): a 2x bitmap shown in a 1x-pt bounds is
/// pixel-perfect on retina displays.
const RASTER_SCALE: u32 = 2;

/// Cache key: one entry per (symbol, pixel size, tint), so the same icon in
/// the same colour never rasterizes twice. Tints are quantized to u8 — a
/// theme has a handful of distinct colours, so the cache stays tiny.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct CacheKey {
    symbol: &'static str,
    width: u32,
    height: u32,
    tint: (u8, u8, u8),
}

fn cache() -> &'static Mutex<HashMap<CacheKey, Arc<RenderImage>>> {
    static CACHE: OnceLock<Mutex<HashMap<CacheKey, Arc<RenderImage>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Renders the SF Symbol named `symbol` at `size_pt` points (in the logical
/// coordinate space GPUI paints in) tinted with `tint` (0..=255 per
/// channel), returning a premultiplied-BGRA [`RenderImage`] sized
/// `size_pt * RASTER_SCALE` pixels.
///
/// Returns `None` when the symbol does not exist on this system (a
/// misspelled name, or a symbol added in a newer macOS), so the caller can
/// fall back to its embedded SVG.
pub(crate) fn rasterize_symbol(
    symbol: &'static str,
    size_pt: f32,
    tint: (u8, u8, u8),
) -> Option<Arc<RenderImage>> {
    let width = (size_pt * RASTER_SCALE as f32).round() as u32;
    let height = width;
    let key = CacheKey {
        symbol,
        width,
        height,
        tint,
    };
    if let Some(image) = cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
    {
        return Some(image);
    }

    let rgba = rasterize_symbol_mask(symbol, width, height)?;
    let bgra = bake_tint(&rgba, width, height, tint);

    let buffer = ImageBuffer::<ImageRgba<u8>, Vec<u8>>::from_raw(width, height, bgra)?;
    let image = Arc::new(RenderImage::new(SmallVec::from_const([Frame::new(buffer)])));
    if let Ok(mut cache) = cache().lock() {
        cache.insert(key, image.clone());
    }
    Some(image)
}

/// The symbol's pixels as straight-alpha RGBA (shape in the alpha channel,
/// black RGB), rasterized by AppKit at exactly `width`x`height` pixels.
///
/// The symbol image is drawn into an `NSBitmapImageRep` of the target
/// size through a temporary `NSGraphicsContext` — the TIFF shortcut cannot
/// be used because `TIFFRepresentation` rasterizes at the symbol's
/// intrinsic size, not the size the image is asked to report.
fn rasterize_symbol_mask(symbol: &str, width: u32, height: u32) -> Option<Vec<u8>> {
    use objc2::AnyThread;
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{
        NSBitmapImageRep, NSCalibratedRGBColorSpace, NSCompositingOperation, NSGraphicsContext,
        NSImage,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

    autoreleasepool(|_| {
        let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(symbol),
            None,
        )?;
        image.setTemplate(true);

        // An owned RGBA8888 bitmap of the exact target size.
        let rep = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                std::ptr::null_mut(),
                width as isize,
                height as isize,
                8,
                4,
                true,
                false,
                NSCalibratedRGBColorSpace,
                0,
                32,
            )
        }?;

        // Draw the vector symbol into the bitmap, scaled to fill.
        let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
        NSGraphicsContext::saveGraphicsState_class();
        NSGraphicsContext::setCurrentContext(Some(&context));
        let target = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: width as f64,
                height: height as f64,
            },
        };
        image.drawInRect_fromRect_operation_fraction(
            target,
            target,
            NSCompositingOperation::Copy,
            1.0,
        );
        NSGraphicsContext::restoreGraphicsState_class();

        let bytes_per_row = rep.bytesPerRow() as usize;
        let samples = rep.samplesPerPixel() as usize;
        let alpha_first = (rep.bitmapFormat().0 & objc2_app_kit::NSBitmapFormat::AlphaFirst.0) != 0;
        if samples < 4 {
            return None;
        }
        // SAFETY: bitmapData is valid for pixelsHigh rows of bytesPerRow
        // bytes; we only read within that.
        let data = unsafe {
            std::slice::from_raw_parts(
                rep.bitmapData() as *const u8,
                bytes_per_row * height as usize,
            )
        };

        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        let alpha_index = if alpha_first { 0 } else { 3 };
        for row in 0..height as usize {
            let row_start = row * bytes_per_row;
            for col in 0..width as usize {
                let px = row_start + col * samples;
                let alpha = data[px + alpha_index];
                // RGB is the symbol's own (black for a template); the alpha
                // mask is what carries the shape.
                rgba.extend_from_slice(&[0, 0, 0, alpha]);
            }
        }
        Some(rgba)
    })
}

/// Multiplies the tint into a straight-alpha RGBA mask, yielding
/// premultiplied BGRA — the exact byte format GPUI's `RenderImage` expects
/// (the SVG path produces the same via `swap_rgba_pa_to_bgra`).
fn bake_tint(rgba: &[u8], width: u32, height: u32, tint: (u8, u8, u8)) -> Vec<u8> {
    let (tr, tg, tb) = tint;
    let mut bgra = Vec::with_capacity(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        let alpha = pixel[3] as u32;
        bgra.push((tb as u32 * alpha / 255) as u8);
        bgra.push((tg as u32 * alpha / 255) as u8);
        bgra.push((tr as u32 * alpha / 255) as u8);
        bgra.push(alpha as u8);
    }
    debug_assert_eq!(bgra.len(), (width * height * 4) as usize);
    bgra
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The SF Symbol pipeline end to end: `folder.fill` must rasterize to a
    /// non-empty, correctly-tinted, correctly-sized bitmap. This is the
    /// milestone test — if SF Symbols were unreachable from Rust, this is
    /// where that fact surfaces with evidence.
    #[test]
    fn folder_fill_rasterizes_with_tint() {
        let Some(image) = rasterize_symbol("folder.fill", 14.0, (255, 128, 0)) else {
            panic!("NSImage(systemSymbolName: \"folder.fill\") produced no bitmap");
        };
        let size = image.size(0);
        assert_eq!(size.width.0, 28, "14pt at 2x is 28 device pixels");
        assert_eq!(size.height.0, 28);

        let bytes = image.as_bytes(0).expect("single frame");
        let mut painted = 0;
        for pixel in bytes.chunks_exact(4) {
            let (b, g, r, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
            if a == 0 {
                continue;
            }
            painted += 1;
            // Premultiplied BGRA tint: blue is zero, red is alpha, and
            // green is the orange tint scaled by alpha. A fully opaque
            // correctly tinted pixel is therefore [0, 128, 255, 255], not
            // an "untinted" pixel.
            assert_eq!(b, 0, "blue channel stays zero in an orange tint");
            assert_eq!(r, a, "red channel is the 255 tint scaled by alpha");
            assert_eq!(g, (128u32 * a as u32 / 255) as u8);
        }
        assert!(
            painted > 100,
            "the folder glyph paints a real shape, got {painted} non-transparent pixels"
        );
    }

    #[test]
    fn bake_tint_produces_premultiplied_bgra() {
        let mask: Vec<u8> = (0..4).flat_map(|_| [0, 0, 0, 255]).collect();
        let out = bake_tint(&mask, 2, 2, (255, 0, 0));
        for pixel in out.chunks_exact(4) {
            assert_eq!(
                pixel,
                &[0, 0, 255, 255],
                "red tint -> BGRA (0, 0, 255, 255)"
            );
        }
    }

    #[test]
    fn unknown_symbol_returns_none() {
        assert!(rasterize_symbol("definitely-not-a-symbol-xyz", 14.0, (255, 255, 255)).is_none());
    }
}
