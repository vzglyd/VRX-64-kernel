//! Platform-agnostic HUD overlay geometry.
//!
//! Provides the vertex type, font-atlas pixel generation, and geometry builder
//! used by all host implementations (native wgpu, web WebGPU) to render a
//! consistent border frame, footer bar, slide title, and wall-clock time on top
//! of every slide.
//!
//! # Design
//!
//! This module is deliberately free of GPU, platform, or runtime dependencies.
//! Callers are responsible for:
//! - Uploading the font atlas pixels to a GPU texture (use [`build_font_atlas_pixels`])
//! - Building geometry each frame (use [`build_hud_geometry`])
//! - Rendering with alpha-blending in a second pass *after* the slide pass

use bytemuck::{Pod, Zeroable};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use std::collections::HashMap;

// ── Vertex type ───────────────────────────────────────────────────────────────

/// Vertex for HUD overlay geometry.
///
/// Memory layout (40 bytes, all `f32`):
/// | offset | field    | type       | description                     |
/// |--------|----------|------------|---------------------------------|
/// | 0      | position | `[f32; 2]` | NDC xy position                 |
/// | 8      | uv       | `[f32; 2]` | font atlas UV (0,0 for solid)   |
/// | 16     | color    | `[f32; 4]` | linear RGBA color               |
/// | 32     | mode     | `f32`      | 0 = solid quad, 1 = font glyph |
/// | 36     | _pad     | `[f32; 1]` | padding for Pod alignment       |
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct OverlayVertex {
    /// NDC xy position in `[-1.0, 1.0]`.
    pub position: [f32; 2],
    /// Font atlas UV coordinates; `[0.0, 0.0]` for solid-colour quads.
    pub uv: [f32; 2],
    /// Linear RGBA colour.
    pub color: [f32; 4],
    /// Rendering mode: `0.0` = solid colour, `1.0` = font glyph (atlas sample).
    pub mode: f32,
    #[doc(hidden)]
    pub _pad: [f32; 1],
}

const MODE_SOLID: f32 = 0.0;
const MODE_FONT: f32 = 1.0;

// ── Styling constants ─────────────────────────────────────────────────────────

/// Border width in pixels.
pub const BORDER_PX: f32 = 4.0;
/// Footer bar height in pixels.
pub const FOOTER_PX: f32 = 24.0;
/// Text scale multiplier (2× → 16 px tall glyphs).
pub const TEXT_SCALE: f32 = 2.0;
/// Base glyph size in pixels (font8x8 is 8×8).
pub const GLYPH_SIZE: f32 = 8.0;
/// Horizontal text margin inside the footer.
pub const TEXT_MARGIN_PX: f32 = 8.0;

/// Accent cyan border colour.
pub const COLOR_BORDER: [f32; 4] = [0.30, 0.75, 0.92, 0.92];
/// Footer background colour (dark semi-transparent).
pub const COLOR_FOOTER_BG: [f32; 4] = [0.02, 0.05, 0.12, 0.88];
/// Slide title text colour.
pub const COLOR_TITLE: [f32; 4] = [0.88, 0.92, 1.0, 1.0];
/// Wall-clock text colour.
pub const COLOR_CLOCK: [f32; 4] = [0.60, 0.82, 0.95, 1.0];

// ── Font atlas ────────────────────────────────────────────────────────────────

/// Build RGBA8 pixel data for the font atlas and a glyph UV map.
///
/// The atlas is a single horizontal strip containing one 8×8 glyph per ASCII
/// character in the range `' '` (32) through `'~'` (126).
///
/// Returns `(rgba_pixels, atlas_width, atlas_height, glyph_map)`.
/// Upload `rgba_pixels` to a GPU `Rgba8Unorm` texture of the returned dimensions.
pub fn build_font_atlas_pixels() -> (Vec<u8>, u32, u32, HashMap<char, [f32; 4]>) {
    let chars: Vec<char> = (32u8..=126u8).map(char::from).collect();
    let glyph_size = 8usize;
    let atlas_w = (chars.len() * glyph_size) as u32;
    let atlas_h = glyph_size as u32;
    let mut pixels = vec![0u8; atlas_w as usize * atlas_h as usize * 4];
    let mut glyph_map: HashMap<char, [f32; 4]> = HashMap::with_capacity(chars.len());

    for (idx, ch) in chars.iter().copied().enumerate() {
        let bitmap = BASIC_FONTS
            .get(ch)
            .unwrap_or_else(|| BASIC_FONTS.get('?').expect("font8x8 must include '?'"));
        let x_base = idx * glyph_size;
        for (row, row_bits) in bitmap.iter().copied().enumerate() {
            for col in 0..glyph_size {
                if (row_bits >> col) & 1 == 0 {
                    continue;
                }
                let atlas_x = x_base + col;
                let px_idx = (row * atlas_w as usize + atlas_x) * 4;
                pixels[px_idx..px_idx + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let u0 = x_base as f32 / atlas_w as f32;
        let u1 = (x_base + glyph_size) as f32 / atlas_w as f32;
        glyph_map.insert(ch, [u0, 0.0, u1, 1.0]);
    }

    (pixels, atlas_w, atlas_h, glyph_map)
}

// ── Geometry builder ──────────────────────────────────────────────────────────

/// Builds HUD overlay vertex + index data for the given surface dimensions.
///
/// Drawing order (painter's algorithm, later = on top):
/// 1. Footer background
/// 2. Slide title text (left-aligned, may be `None`)
/// 3. Clock text (right-aligned)
/// 4. Border strips (topmost layer)
///
/// # Parameters
/// - `glyph_map`: from [`build_font_atlas_pixels`]
/// - `sw`, `sh`: surface width and height in pixels
/// - `slide_name`: optional slide name shown in the footer
/// - `clock_str`: pre-formatted wall-clock string (e.g. `"14:30:05"`)
pub fn build_hud_geometry(
    glyph_map: &HashMap<char, [f32; 4]>,
    sw: u32,
    sh: u32,
    slide_name: Option<&str>,
    clock_str: &str,
) -> (Vec<OverlayVertex>, Vec<u16>) {
    let mut verts: Vec<OverlayVertex> = Vec::new();
    let mut idxs: Vec<u16> = Vec::new();

    let sw = sw as f32;
    let sh = sh as f32;
    let advance = GLYPH_SIZE * TEXT_SCALE;
    let text_y = sh - FOOTER_PX + (FOOTER_PX - advance) * 0.5;

    // 1. Footer background
    push_solid(
        &mut verts,
        &mut idxs,
        0.0,
        sh - FOOTER_PX,
        sw,
        sh,
        sw,
        sh,
        COLOR_FOOTER_BG,
    );

    // 2. Slide title (left-aligned, capped to avoid overflow into clock area)
    if let Some(name) = slide_name {
        let name = normalize_text(name);
        let max_title_chars = ((sw * 0.5 - TEXT_MARGIN_PX * 2.0) / advance).floor() as usize;
        let truncated: String = name.chars().take(max_title_chars).collect();
        push_text(
            &mut verts,
            &mut idxs,
            glyph_map,
            &truncated,
            TEXT_MARGIN_PX,
            text_y,
            advance,
            sw,
            sh,
            COLOR_TITLE,
        );
    }

    // 3. Clock (right-aligned)
    let clock_width = clock_str.chars().count() as f32 * advance;
    let clock_x = sw - TEXT_MARGIN_PX - clock_width;
    push_text(
        &mut verts,
        &mut idxs,
        glyph_map,
        clock_str,
        clock_x,
        text_y,
        advance,
        sw,
        sh,
        COLOR_CLOCK,
    );

    // 4. Border strips (drawn last — on top of everything)
    push_solid(&mut verts, &mut idxs, 0.0, 0.0, sw, BORDER_PX, sw, sh, COLOR_BORDER);
    push_solid(&mut verts, &mut idxs, 0.0, sh - BORDER_PX, sw, sh, sw, sh, COLOR_BORDER);
    push_solid(&mut verts, &mut idxs, 0.0, 0.0, BORDER_PX, sh, sw, sh, COLOR_BORDER);
    push_solid(&mut verts, &mut idxs, sw - BORDER_PX, 0.0, sw, sh, sw, sh, COLOR_BORDER);

    (verts, idxs)
}

// ── Text normalization ────────────────────────────────────────────────────────

/// Map common Unicode typographic characters to ASCII equivalents.
///
/// Non-ASCII characters that cannot be mapped are replaced with `'?'`.
pub fn normalize_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\u{2018}' | '\u{2019}' => out.push('\''),
            '\u{201C}' | '\u{201D}' => out.push('"'),
            '\u{2013}' | '\u{2014}' | '\u{2212}' => out.push('-'),
            '\u{2026}' => out.push_str("..."),
            ch if ch.is_ascii() && !ch.is_ascii_control() => out.push(ch),
            _ => out.push('?'),
        }
    }
    out
}

// ── Quad helpers ──────────────────────────────────────────────────────────────

#[inline]
fn px_to_ndc_x(px: f32, sw: f32) -> f32 {
    2.0 * px / sw - 1.0
}

#[inline]
fn px_to_ndc_y(py: f32, sh: f32) -> f32 {
    1.0 - 2.0 * py / sh
}

/// Push a solid-colour quad from pixel-space rect `(x0, y0) → (x1, y1)`.
fn push_solid(
    verts: &mut Vec<OverlayVertex>,
    idxs: &mut Vec<u16>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    sw: f32,
    sh: f32,
    color: [f32; 4],
) {
    let nx0 = px_to_ndc_x(x0, sw);
    let nx1 = px_to_ndc_x(x1, sw);
    let ny0 = px_to_ndc_y(y0, sh);
    let ny1 = px_to_ndc_y(y1, sh);
    let base = verts.len() as u16;
    verts.extend_from_slice(&[
        OverlayVertex { position: [nx0, ny0], uv: [0.0, 0.0], color, mode: MODE_SOLID, _pad: [0.0] },
        OverlayVertex { position: [nx1, ny0], uv: [0.0, 0.0], color, mode: MODE_SOLID, _pad: [0.0] },
        OverlayVertex { position: [nx1, ny1], uv: [0.0, 0.0], color, mode: MODE_SOLID, _pad: [0.0] },
        OverlayVertex { position: [nx0, ny1], uv: [0.0, 0.0], color, mode: MODE_SOLID, _pad: [0.0] },
    ]);
    idxs.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Push font-atlas glyph quads for `text` starting at pixel `(x, y)`.
fn push_text(
    verts: &mut Vec<OverlayVertex>,
    idxs: &mut Vec<u16>,
    glyph_map: &HashMap<char, [f32; 4]>,
    text: &str,
    x: f32,
    y: f32,
    advance: f32,
    sw: f32,
    sh: f32,
    color: [f32; 4],
) {
    for (i, ch) in text.chars().enumerate() {
        let uvs = match glyph_map.get(&ch).or_else(|| glyph_map.get(&'?')) {
            Some(u) => *u,
            None => continue,
        };
        let [u0, v0, u1, v1] = uvs;
        let gx0 = x + i as f32 * advance;
        let gx1 = gx0 + advance;
        let gy0 = y;
        let gy1 = y + advance;

        let nx0 = px_to_ndc_x(gx0, sw);
        let nx1 = px_to_ndc_x(gx1, sw);
        let ny0 = px_to_ndc_y(gy0, sh);
        let ny1 = px_to_ndc_y(gy1, sh);

        let base = verts.len() as u16;
        // TL, TR, BR, BL — UV v-axis matches pixel y-axis (v0=top, v1=bottom)
        verts.extend_from_slice(&[
            OverlayVertex { position: [nx0, ny0], uv: [u0, v0], color, mode: MODE_FONT, _pad: [0.0] },
            OverlayVertex { position: [nx1, ny0], uv: [u1, v0], color, mode: MODE_FONT, _pad: [0.0] },
            OverlayVertex { position: [nx1, ny1], uv: [u1, v1], color, mode: MODE_FONT, _pad: [0.0] },
            OverlayVertex { position: [nx0, ny1], uv: [u0, v1], color, mode: MODE_FONT, _pad: [0.0] },
        ]);
        idxs.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_glyph_map() -> HashMap<char, [f32; 4]> {
        let chars: Vec<char> = (32u8..=126u8).map(char::from).collect();
        let n = chars.len();
        let mut m = HashMap::new();
        for (i, ch) in chars.iter().copied().enumerate() {
            let u0 = i as f32 / n as f32;
            let u1 = (i + 1) as f32 / n as f32;
            m.insert(ch, [u0, 0.0, u1, 1.0]);
        }
        m
    }

    #[test]
    fn font_atlas_pixels_cover_ascii() {
        let (pixels, w, h, glyph_map) = build_font_atlas_pixels();
        assert_eq!(h, 8);
        assert_eq!(pixels.len(), w as usize * h as usize * 4);
        for ch in (32u8..=126u8).map(char::from) {
            assert!(glyph_map.contains_key(&ch), "missing glyph for '{ch}'");
        }
        for ch in ['0', '9', 'A', 'Z', ':', ' '] {
            assert!(glyph_map.contains_key(&ch));
        }
    }

    #[test]
    fn build_hud_geometry_non_empty() {
        let m = test_glyph_map();
        let (verts, idxs) = build_hud_geometry(&m, 640, 480, Some("Test Slide"), "12:34:56");
        assert!(!verts.is_empty());
        assert!(!idxs.is_empty());
    }

    #[test]
    fn build_hud_geometry_positions_in_ndc() {
        let m = test_glyph_map();
        let (verts, _) = build_hud_geometry(&m, 640, 480, Some("My Slide"), "00:00:00");
        for v in &verts {
            assert!(
                v.position[0] >= -1.0 && v.position[0] <= 1.0,
                "x out of NDC: {}",
                v.position[0]
            );
            assert!(
                v.position[1] >= -1.0 && v.position[1] <= 1.0,
                "y out of NDC: {}",
                v.position[1]
            );
        }
    }

    #[test]
    fn build_hud_geometry_no_slide_name() {
        let m = test_glyph_map();
        let (verts, idxs) = build_hud_geometry(&m, 1920, 1080, None, "23:59:59");
        assert!(!verts.is_empty());
        assert!(!idxs.is_empty());
    }

    #[test]
    fn footer_background_at_bottom() {
        let m = test_glyph_map();
        let sh = 480u32;
        let sw = 640u32;
        let (verts, _) = build_hud_geometry(&m, sw, sh, None, "00:00:00");

        let min_y = verts.iter().map(|v| v.position[1]).fold(f32::INFINITY, f32::min);
        assert!((min_y - (-1.0)).abs() < 1e-5, "footer bottom y = {min_y}");

        let footer_top_ndc = 1.0 - 2.0 * (sh as f32 - FOOTER_PX) / sh as f32;
        let has_footer_top = verts
            .iter()
            .any(|v| (v.position[1] - footer_top_ndc).abs() < 1e-4);
        assert!(has_footer_top, "no vertex at footer top ndc={footer_top_ndc}");
    }

    #[test]
    fn normalize_text_strips_control() {
        assert_eq!(normalize_text("hello\x00world"), "hello?world");
        assert_eq!(normalize_text("abc"), "abc");
        assert_eq!(normalize_text("\u{2014}"), "-");
    }
}
