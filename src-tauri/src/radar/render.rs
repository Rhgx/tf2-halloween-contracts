//! Drawing in TF2's HUD style: a dark tan panel with a ground compass in perspective (bright front edge, dim
//! back edge), an arrow lying in that plane, a height pin, and TF2's own fonts with drop shadows.
//! Layout numbers are in 1080p panel units, multiplied by `k` (screen height / 1080 times the user scale).

use crate::radar::nav::Reading;
use fontdue::Font;
use std::path::{Path, PathBuf};
use tiny_skia::{
    Color, FillRule, LineCap, Paint, Path as SkPath, PathBuilder, Pixmap, Stroke, StrokeDash,
    Transform,
};

pub const PANEL_W: f32 = 232.0;
pub const PANEL_H: f32 = 100.0;
/// The grip square in the panel's bottom-right corner (edit mode).
pub const GRIP: f32 = 12.0;

type Rgb = [u8; 3];
const CREAM: Rgb = [0xEB, 0xE3, 0xCB];
const TAN_DARK: Rgb = [0x75, 0x6B, 0x5E];
/// Dimmed text (below / level): lighter than TAN_DARK so it stays readable on the panel.
const TAN_TEXT: Rgb = [0x9E, 0x93, 0x80];
const PANEL_BG: Rgb = [0x2E, 0x2B, 0x2A];
const ORANGE: Rgb = [0xE5, 0x86, 0x26];
const RIM: Rgb = [0x1A, 0x18, 0x16];
const BLACK: Rgb = [0, 0, 0];
const SHADOW_ALPHA: f32 = 0.6;

const LEVEL_M: f32 = 1.5;
const ARRIVED_M: f32 = 2.5;
const RING_CX: f32 = 56.0;
const RING_CY: f32 = 50.0;
const RING_RX: f32 = 40.0;
const RING_RY: f32 = 18.0; // 0.45 of rx: a compass on the floor seen from about 27 degrees above
const PIN_PX_PER_M: f32 = 4.0;
const PIN_MAX: f32 = 24.0;
const TEXT_X: f32 = 112.0;

/// Window size for the panel, with room for the drop shadow.
pub fn panel_pixels(k: f32) -> (u32, u32) {
    (
        (PANEL_W * k + 2.0 * k).ceil() as u32,
        (PANEL_H * k + 2.0 * k).ceil() as u32,
    )
}

pub struct Painter {
    big: Font,
    small: Font,
    pub tf2_fonts: bool,
}

#[derive(Clone, Copy)]
enum Align {
    Left,
    Right,
}

impl Painter {
    /// TF2's own HUD fonts from `tf\resource` when available, otherwise Segoe UI / Arial.
    pub fn load(tf_dir: Option<&Path>) -> Result<Painter, String> {
        let res = tf_dir.map(|d| d.join("resource"));
        let load = |candidates: &[Option<PathBuf>]| -> Option<(Font, bool)> {
            candidates.iter().enumerate().find_map(|(i, p)| {
                let bytes = std::fs::read(p.as_ref()?).ok()?;
                Font::from_bytes(bytes, fontdue::FontSettings::default())
                    .ok()
                    .map(|f| (f, i == 0))
            })
        };
        let win = |name: &str| Some(PathBuf::from(r"C:\Windows\Fonts").join(name));
        let big = load(&[
            res.as_ref().map(|r| r.join("tf2build.ttf")),
            win("segoeuib.ttf"),
            win("arialbd.ttf"),
        ]);
        let small = load(&[
            res.as_ref().map(|r| r.join("tf2secondary.ttf")),
            win("segoeui.ttf"),
            win("arial.ttf"),
        ]);
        match (big, small) {
            (Some((big, a)), Some((small, b))) => Ok(Painter {
                big,
                small,
                tf2_fonts: a && b,
            }),
            _ => Err("no usable font found (TF2's resource folder or C:\\Windows\\Fonts)".into()),
        }
    }

    /// The radar panel. `edit` adds the dashed outline and resize grip.
    pub fn panel(&self, pm: &mut Pixmap, k: f32, opacity: f32, r: &Reading, edit: bool) {
        pm.fill(Color::TRANSPARENT);
        let t = Transform::from_scale(k, k);
        let bg = rounded_rect(0.0, 0.0, PANEL_W, PANEL_H, 4.0);
        fill(pm, &bg, PANEL_BG, opacity, t, k);

        let arrived = r.dist_m < ARRIVED_M;
        let accent = if arrived { ORANGE } else { CREAM };
        let flat = RING_RY / RING_RX;
        // the arrow lies on the ground plane: forward is up the screen, foreshortened by `flat`
        let (sin, cos) = r.angle_deg.to_radians().sin_cos();
        let ground = |side: f32, fwd: f32| (RING_CX + side, RING_CY - fwd * flat);
        let tip = if arrived {
            (RING_CX, RING_CY)
        } else {
            ground(sin * 0.85 * RING_RX, cos * 0.85 * RING_RX)
        };
        let level = r.height_m.abs() < LEVEL_M;
        let pin_len = if level {
            0.0
        } else {
            (r.height_m * PIN_PX_PER_M).clamp(-PIN_MAX, PIN_MAX)
        };

        // below you: the pin hangs under the floor, so the ring is drawn over it
        if pin_len < 0.0 {
            pin(pm, tip, pin_len, accent, t, k);
        }
        stroke(pm, &half_ellipse(true), TAN_DARK, 2.5, None, t, k);
        stroke(
            pm,
            &line(
                (RING_CX, RING_CY - RING_RY),
                (RING_CX, RING_CY - RING_RY - 6.0),
            ),
            TAN_DARK,
            2.0,
            None,
            t,
            k,
        );
        stroke(pm, &half_ellipse(false), accent, 2.5, None, t, k);
        if !arrived {
            let base = ground(-sin * 0.35 * RING_RX, -cos * 0.35 * RING_RX);
            // the width is only partly foreshortened, or the dart turns into a sliver at diagonal angles
            // perpendicular in the ground plane is (cos, -sin); forward maps to screen -y, so the wing's y is +sin
            let (wx, wy) = (cos * 0.42 * RING_RX, sin * 0.42 * RING_RX * flat.sqrt());
            let dart = polygon(&[
                tip,
                (base.0 + wx, base.1 + wy),
                (RING_CX, RING_CY),
                (base.0 - wx, base.1 - wy),
            ]);
            fill(pm, &dart, CREAM, 1.0, t, k);
        }
        if pin_len >= 0.0 {
            pin(pm, tip, pin_len, accent, t, k);
        }

        let distance = if arrived {
            "HERE".to_string()
        } else if r.dist_m < 10.0 {
            format!("{:.1} m", r.dist_m)
        } else {
            format!("{:.0} m", r.dist_m)
        };
        self.text(
            pm,
            &self.big,
            TEXT_X * k,
            44.0 * k,
            &distance,
            30.0 * k,
            accent,
            Align::Left,
            k,
        );
        let (height, colour) = if arrived {
            let look = if level {
                "level"
            } else if r.height_m > 0.0 {
                "look up"
            } else {
                "look down"
            };
            (look.to_string(), ORANGE)
        } else if level {
            ("level".to_string(), TAN_TEXT)
        } else {
            let m = r.height_m.abs();
            let amount = if m < 10.0 {
                format!("{m:.1}")
            } else {
                format!("{m:.0}")
            };
            // below is where you cannot see it: dimmed, so above and below differ at a glance
            if r.height_m > 0.0 {
                (format!("{amount} m above"), CREAM)
            } else {
                (format!("{amount} m below"), TAN_TEXT)
            }
        };
        self.text(
            pm,
            &self.small,
            TEXT_X * k,
            70.0 * k,
            &height,
            15.0 * k,
            colour,
            Align::Left,
            k,
        );

        if edit {
            let outline = rounded_rect(0.5, 0.5, PANEL_W - 1.0, PANEL_H - 1.0, 4.0);
            stroke(
                pm,
                &outline,
                CREAM,
                1.0,
                StrokeDash::new(vec![4.0, 3.0], 0.0),
                t,
                0.0,
            );
            let grip = polygon(&[
                (PANEL_W, PANEL_H - GRIP),
                (PANEL_W, PANEL_H),
                (PANEL_W - GRIP, PANEL_H),
            ]);
            fill(pm, &grip, CREAM, 1.0, t, 0.0);
        }
    }

    /// Help card shown under the panel in edit mode.
    /// `save_key` names the hotkey that saves and leaves edit mode.
    pub fn help_card(&self, k: f32, opacity: f32, save_key: &str) -> Pixmap {
        let last = format!(
            "Arrows nudge (Shift: 10 px), + / - resize, PgUp / PgDn opacity, Home resets, {save_key} saves."
        );
        let lines = [
            "EDIT MODE",
            "Press Esc in TF2 to free the mouse. Drag to move, drag the corner to resize.",
            last.as_str(),
        ];
        let px = 14.0 * k;
        let width = lines
            .iter()
            .map(|l| self.measure(&self.small, l, px))
            .fold(0.0, f32::max);
        let (pad, line_h) = (10.0 * k, 20.0 * k);
        let (w, h) = (
            (width + 2.0 * pad + 2.0 * k).ceil() as u32,
            (lines.len() as f32 * line_h + 2.0 * pad).ceil() as u32,
        );
        let mut pm = Pixmap::new(w, h).unwrap();
        fill(
            &mut pm,
            &rounded_rect(0.0, 0.0, w as f32 - 2.0 * k, h as f32 - 2.0 * k, 4.0 * k),
            PANEL_BG,
            opacity,
            Transform::identity(),
            k,
        );
        for (i, line) in lines.iter().enumerate() {
            let colour = if i == 0 { CREAM } else { [0xC8, 0xC0, 0xAA] };
            self.text(
                &mut pm,
                &self.small,
                pad,
                pad + (i as f32 + 0.75) * line_h,
                line,
                px,
                colour,
                Align::Left,
                k,
            );
        }
        pm
    }

    /// Status text anchored at the bottom-right of the pixmap.
    pub fn corner(&self, pm: &mut Pixmap, text: &str, k: f32) {
        pm.fill(Color::TRANSPARENT);
        let (w, h) = (pm.width() as f32, pm.height() as f32);
        self.text(
            pm,
            &self.small,
            w - 3.0 * k,
            h - 8.0 * k,
            text,
            15.0 * k,
            CREAM,
            Align::Right,
            k,
        );
    }

    fn measure(&self, font: &Font, text: &str, px: f32) -> f32 {
        text.chars()
            .map(|c| font.metrics(c, px).advance_width)
            .sum()
    }

    /// Text on a baseline, with TF2's drop shadow (the same text in black, offset down-right).
    #[allow(clippy::too_many_arguments)]
    fn text(
        &self,
        pm: &mut Pixmap,
        font: &Font,
        x: f32,
        baseline: f32,
        text: &str,
        px: f32,
        colour: Rgb,
        align: Align,
        k: f32,
    ) {
        let px = px.max(8.0);
        let glyphs: Vec<_> = text.chars().map(|c| font.rasterize(c, px)).collect();
        let width: f32 = glyphs.iter().map(|(m, _)| m.advance_width).sum();
        let left = match align {
            Align::Left => x,
            Align::Right => x - width,
        };
        let shadow = k.round().max(1.0) as i32;
        for (dx, c, alpha) in [(shadow, BLACK, SHADOW_ALPHA), (0, colour, 1.0)] {
            let mut pen = left;
            for (m, bitmap) in &glyphs {
                let gx = (pen + m.xmin as f32).round() as i32 + dx;
                let gy = (baseline - m.height as f32 - m.ymin as f32).round() as i32 + dx;
                blend_coverage(pm, gx, gy, m.width, m.height, bitmap, c, alpha);
                pen += m.advance_width;
            }
        }
    }
}

/// Height pin from the arrow tip: a stalk of `len` panel units (positive = up) ending in a rimmed dot.
fn pin(pm: &mut Pixmap, tip: (f32, f32), len: f32, colour: Rgb, t: Transform, k: f32) {
    let end = (tip.0, tip.1 - len);
    if len != 0.0 {
        stroke(pm, &line(tip, end), colour, 2.0, None, t, k);
    }
    let dot = PathBuilder::from_circle(end.0, end.1, 2.5).unwrap();
    fill(pm, &dot, colour, 1.0, t, k);
    stroke(pm, &dot, RIM, 1.0, None, t, 0.0);
}

fn paint(colour: Rgb, alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(
        colour[0],
        colour[1],
        colour[2],
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    );
    p.anti_alias = true;
    p
}

/// Shadow offset in pixels for `k` (0 disables the shadow).
fn shadowed(t: Transform, k: f32) -> Option<Transform> {
    (k > 0.0).then(|| t.post_translate(k.round().max(1.0), k.round().max(1.0)))
}

fn fill(pm: &mut Pixmap, path: &SkPath, colour: Rgb, alpha: f32, t: Transform, k: f32) {
    if let Some(st) = shadowed(t, k) {
        pm.fill_path(
            path,
            &paint(BLACK, SHADOW_ALPHA * alpha),
            FillRule::Winding,
            st,
            None,
        );
    }
    pm.fill_path(path, &paint(colour, alpha), FillRule::Winding, t, None);
}

fn stroke(
    pm: &mut Pixmap,
    path: &SkPath,
    colour: Rgb,
    width: f32,
    dash: Option<StrokeDash>,
    t: Transform,
    k: f32,
) {
    let st = Stroke {
        width,
        line_cap: LineCap::Round,
        dash,
        ..Stroke::default()
    };
    if let Some(shadow_t) = shadowed(t, k) {
        pm.stroke_path(path, &paint(BLACK, SHADOW_ALPHA), &st, shadow_t, None);
    }
    pm.stroke_path(path, &paint(colour, 1.0), &st, t, None);
}

fn line(a: (f32, f32), b: (f32, f32)) -> SkPath {
    let mut pb = PathBuilder::new();
    pb.move_to(a.0, a.1);
    pb.line_to(b.0, b.1);
    pb.finish().unwrap()
}

fn polygon(points: &[(f32, f32)]) -> SkPath {
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].0, points[0].1);
    for p in &points[1..] {
        pb.line_to(p.0, p.1);
    }
    pb.close();
    pb.finish().unwrap()
}

/// Back (upper) or front (lower) half of the compass ellipse.
fn half_ellipse(back: bool) -> SkPath {
    let start = if back { std::f32::consts::PI } else { 0.0 };
    let mut pb = PathBuilder::new();
    for i in 0..=32 {
        let a = start + std::f32::consts::PI * i as f32 / 32.0;
        let p = (RING_CX + RING_RX * a.cos(), RING_CY + RING_RY * a.sin());
        if i == 0 {
            pb.move_to(p.0, p.1)
        } else {
            pb.line_to(p.0, p.1)
        }
    }
    pb.finish().unwrap()
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> SkPath {
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}

/// "Source over" of a glyph coverage mask in a solid colour onto the premultiplied pixmap.
#[allow(clippy::too_many_arguments)]
fn blend_coverage(
    pm: &mut Pixmap,
    x0: i32,
    y0: i32,
    w: usize,
    h: usize,
    coverage: &[u8],
    colour: Rgb,
    alpha: f32,
) {
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let data = pm.data_mut();
    for gy in 0..h as i32 {
        let y = y0 + gy;
        if y < 0 || y >= ph {
            continue;
        }
        for gx in 0..w as i32 {
            let x = x0 + gx;
            let a = (coverage[(gy * w as i32 + gx) as usize] as f32 * alpha) as u32;
            if x < 0 || x >= pw || a == 0 {
                continue;
            }
            let i = ((y * pw + x) * 4) as usize;
            for c in 0..3 {
                data[i + c] = ((colour[c] as u32 * a + data[i + c] as u32 * (255 - a)) / 255) as u8;
            }
            data[i + 3] = (a + data[i + 3] as u32 * (255 - a) / 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders the states onto a dark and a bright strip and writes radar_preview.bmp to the temp folder.
    /// Run with: cargo test --release -- --ignored preview
    #[test]
    #[ignore]
    fn preview() {
        let tf = PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Team Fortress 2\tf");
        let painter = Painter::load(Some(&tf)).unwrap();
        let k = 1.5;
        let states = [
            (
                Reading {
                    angle_deg: 0.0,
                    dist_m: 63.0,
                    height_m: 0.4,
                },
                false,
            ),
            (
                Reading {
                    angle_deg: 60.0,
                    dist_m: 8.4,
                    height_m: 4.6,
                },
                false,
            ),
            (
                Reading {
                    angle_deg: -140.0,
                    dist_m: 31.0,
                    height_m: -12.0,
                },
                false,
            ),
            (
                Reading {
                    angle_deg: 95.0,
                    dist_m: 18.0,
                    height_m: -3.0,
                },
                false,
            ),
            (
                Reading {
                    angle_deg: 0.0,
                    dist_m: 1.6,
                    height_m: 2.2,
                },
                false,
            ),
            (
                Reading {
                    angle_deg: 35.0,
                    dist_m: 48.0,
                    height_m: 4.0,
                },
                true,
            ),
        ];
        let (pw, ph) = panel_pixels(k);
        let (tile_w, tile_h) = (pw as usize + 20, ph as usize + 20);
        let (out_w, out_h) = (tile_w * states.len(), tile_h * 2);
        let mut out = vec![[0u8; 3]; out_w * out_h];
        for (row, bg) in [[32u8, 30, 28], [176, 168, 144]].iter().enumerate() {
            for (i, (r, edit)) in states.iter().enumerate() {
                let mut pm = Pixmap::new(pw, ph).unwrap();
                painter.panel(&mut pm, k, 0.78, r, *edit);
                for y in 0..tile_h {
                    for x in 0..tile_w {
                        let mut c = *bg;
                        if (10..10 + ph as usize).contains(&y)
                            && (10..10 + pw as usize).contains(&x)
                        {
                            let p = &pm.data()[((y - 10) * pw as usize + x - 10) * 4..][..4];
                            for ch in 0..3 {
                                c[ch] =
                                    (p[ch] as u32 + c[ch] as u32 * (255 - p[3] as u32) / 255) as u8;
                            }
                        }
                        out[(row * tile_h + y) * out_w + i * tile_w + x] = c;
                    }
                }
            }
        }
        let stride = (out_w * 3 + 3) & !3;
        let mut body = Vec::new();
        for y in (0..out_h).rev() {
            for p in &out[y * out_w..(y + 1) * out_w] {
                body.extend_from_slice(&[p[2], p[1], p[0]]);
            }
            body.resize(body.len() + stride - out_w * 3, 0);
        }
        let mut bmp = b"BM".to_vec();
        for v in [
            54 + body.len() as u32,
            0,
            54,
            40,
            out_w as u32,
            out_h as u32,
        ] {
            bmp.extend_from_slice(&v.to_le_bytes());
        }
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&24u16.to_le_bytes());
        for v in [0u32, body.len() as u32, 2835, 2835, 0, 0] {
            bmp.extend_from_slice(&v.to_le_bytes());
        }
        bmp.extend_from_slice(&body);
        std::fs::write(std::env::temp_dir().join("radar_preview.bmp"), bmp).unwrap();
        println!("tf2 fonts: {}", painter.tf2_fonts);
    }
}
