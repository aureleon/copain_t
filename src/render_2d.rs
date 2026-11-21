use eframe::egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Rounding, Stroke, Vec2};

use crate::protocol::DrawStyle;
use crate::scene::{PointerState, Primitive2D, SceneStore, Shape2D};

/// Deterministic fast PRNG so sketchy jitter never flickers between frames.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1))
    }

    fn next_u64(&mut self) -> u64 {
        let mut z = self.0;
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Returns a float in [-1.0, 1.0]
    fn signed(&mut self) -> f32 {
        let u = (self.next_u64() >> 40) as f32 / ((1u64 << 24) as f32);
        u * 2.0 - 1.0
    }
}

fn to_screen(rect: Rect, nx: f32, ny: f32) -> Pos2 {
    Pos2::new(
        rect.left() + nx * rect.width(),
        rect.top() + ny * rect.height(),
    )
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub fn render_2d_layer(painter: &Painter, viewport: Rect, scene: &SceneStore) {
    // 1. Agent-authored backdrop panels (rendered behind all 2D/3D content)
    for bd in &scene.backdrops {
        let p = ease_out_cubic(bd.progress);
        let p0 = to_screen(viewport, bd.x, bd.y);
        let p1 = to_screen(viewport, bd.x + bd.w, bd.y + bd.h);
        let r = Rect::from_two_pos(p0, p1);
        let alpha = ((bd.color.a() as f32) * p) as u8;
        // Soft drop shadow
        painter.rect_filled(
            r.translate(Vec2::new(0.0, 6.0)).expand(1.0),
            Rounding::same(bd.radius + 2.0),
            Color32::from_rgba_unmultiplied(12, 16, 28, (36.0 * p) as u8),
        );
        // Frosted panel fill
        painter.rect_filled(
            r,
            Rounding::same(bd.radius),
            Color32::from_rgba_unmultiplied(bd.color.r(), bd.color.g(), bd.color.b(), alpha),
        );
        // Subtle crisp border
        painter.rect_stroke(
            r,
            Rounding::same(bd.radius),
            Stroke::new(
                1.0_f32,
                Color32::from_rgba_unmultiplied(140, 148, 165, (65.0 * p) as u8),
            ),
        );
    }

    // 2. 2D sketchy shapes, strokes, arrows, and labels
    for prim in &scene.primitives_2d {
        let p = ease_out_cubic(prim.progress);
        draw_primitive(painter, viewport, prim, p);
    }

    if let Some(ptr) = &scene.pointer {
        draw_pointer(painter, viewport, ptr);
    }
}

fn draw_primitive(painter: &Painter, viewport: Rect, prim: &Primitive2D, progress: f32) {
    let mut rng = Rng::new(prim.seed);
    match &prim.shape {
        Shape2D::Rect {
            x,
            y,
            w,
            h,
            color,
            style,
            label,
        } => {
            let p0 = to_screen(viewport, *x, *y);
            let p1 = to_screen(viewport, *x + *w, *y);
            let p2 = to_screen(viewport, *x + *w, *y + *h);
            let p3 = to_screen(viewport, *x, *y + *h);
            let r = Rect::from_two_pos(p0, p2);

            // Soft card backdrop + subtle translucent accent tint
            let card_alpha = (210.0 * progress) as u8;
            painter.rect_filled(
                r.translate(Vec2::new(0.0, 3.0)),
                Rounding::same(10.0),
                Color32::from_rgba_unmultiplied(15, 20, 35, (18.0 * progress) as u8),
            );
            painter.rect_filled(
                r,
                Rounding::same(10.0),
                Color32::from_rgba_unmultiplied(255, 255, 255, card_alpha),
            );
            let fill_alpha = (16.0 * progress) as u8;
            let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), fill_alpha);
            painter.rect_filled(r, Rounding::same(10.0), fill);

            let edges = [(p0, p1), (p1, p2), (p2, p3), (p3, p0)];
            for (idx, (a, b)) in edges.iter().enumerate() {
                let seg_prog = (progress * 4.0 - idx as f32).clamp(0.0, 1.0);
                if seg_prog <= 0.0 {
                    continue;
                }
                match style {
                    DrawStyle::Sketch => {
                        draw_sketch_line(painter, *a, *b, *color, 2.4, seg_prog, &mut rng);
                    }
                    DrawStyle::Solid => {
                        let end = a.lerp(*b, seg_prog);
                        painter.line_segment([*a, end], Stroke::new(2.4_f32, *color));
                    }
                }
            }

            if let Some(text) = label {
                if progress > 0.3 {
                    let alpha = (((progress - 0.3) / 0.7).clamp(0.0, 1.0) * 255.0) as u8;
                    draw_badge(
                        painter,
                        Pos2::new(p0.x + 10.0, p0.y - 2.0),
                        text,
                        *color,
                        alpha,
                    );
                }
            }
        }
        Shape2D::Circle {
            x,
            y,
            r,
            color,
            style,
            label,
        } => {
            let center = to_screen(viewport, *x, *y);
            let radius = *r * viewport.width().min(viewport.height());
            let fill_alpha = (20.0 * progress) as u8;
            let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), fill_alpha);
            painter.circle_filled(center, radius * progress, fill);

            let passes = if *style == DrawStyle::Sketch { 2 } else { 1 };
            let steps = 48usize;
            let max_step = ((steps as f32) * progress).ceil() as usize;

            for pass in 0..passes {
                let jitter = if *style == DrawStyle::Sketch {
                    1.6 + pass as f32 * 0.8
                } else {
                    0.0
                };
                let mut pts = Vec::with_capacity(max_step + 1);
                let start_angle = rng.signed() * 0.3;
                let rx = radius * (1.0 + rng.signed() * 0.03 * jitter.min(1.0));
                let ry = radius * (1.0 + rng.signed() * 0.03 * jitter.min(1.0));
                for i in 0..=max_step {
                    let t = i as f32 / steps as f32;
                    let ang = start_angle + t * std::f32::consts::TAU * 1.03;
                    let jx = rng.signed() * jitter * 0.4;
                    let jy = rng.signed() * jitter * 0.4;
                    pts.push(Pos2::new(
                        center.x + ang.cos() * rx + jx,
                        center.y + ang.sin() * ry + jy,
                    ));
                }
                if pts.len() >= 2 {
                    let stroke_w = if pass == 0 { 2.4_f32 } else { 1.4_f32 };
                    let c = if pass == 0 {
                        *color
                    } else {
                        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 170)
                    };
                    painter.add(eframe::egui::Shape::line(pts, Stroke::new(stroke_w, c)));
                }
            }

            if let Some(text) = label {
                if progress > 0.4 {
                    let alpha = (((progress - 0.4) / 0.6).clamp(0.0, 1.0) * 255.0) as u8;
                    draw_badge(
                        painter,
                        Pos2::new(center.x, center.y - radius - 14.0),
                        text,
                        *color,
                        alpha,
                    );
                }
            }
        }
        Shape2D::Arrow {
            from,
            to,
            color,
            label,
        } => {
            let p0 = to_screen(viewport, from[0], from[1]);
            let p1 = to_screen(viewport, to[0], to[1]);
            let shaft_prog = (progress / 0.78).clamp(0.0, 1.0);
            draw_sketch_line(painter, p0, p1, *color, 2.6, shaft_prog, &mut rng);

            if progress > 0.72 {
                let head_prog = ((progress - 0.72) / 0.28).clamp(0.0, 1.0);
                let dir = (p1 - p0).normalized();
                let perp = Vec2::new(-dir.y, dir.x);
                let head_len = 14.0;
                let head_w = 8.0;
                let wing1 = p1 - dir * head_len + perp * head_w;
                let wing2 = p1 - dir * head_len - perp * head_w;
                draw_sketch_line(painter, p1, wing1, *color, 2.5, head_prog, &mut rng);
                draw_sketch_line(painter, p1, wing2, *color, 2.5, head_prog, &mut rng);
            }

            if let Some(text) = label {
                if progress > 0.4 {
                    let mid = p0.lerp(p1, 0.45) + Vec2::new(0.0, -18.0);
                    let alpha = (((progress - 0.4) / 0.6).clamp(0.0, 1.0) * 255.0) as u8;
                    draw_centered_badge(painter, mid, text, *color, alpha);
                }
            }
        }
        Shape2D::Stroke {
            points,
            color,
            width,
        } => {
            if points.len() < 2 {
                return;
            }
            let screen_pts: Vec<Pos2> = points
                .iter()
                .map(|pt| to_screen(viewport, pt[0], pt[1]))
                .collect();
            draw_tapered_stroke(painter, &screen_pts, *color, *width, progress);
        }
        Shape2D::Label {
            x,
            y,
            text,
            color,
            size,
        } => {
            let pos = to_screen(viewport, *x, *y);
            let alpha = (progress * 255.0) as u8;
            let c = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
            // Subtle readable backdrop in case of overlay mode
            let galley = painter.layout_no_wrap(
                text.clone(),
                FontId::proportional(*size),
                c,
            );
            let bg_rect = Rect::from_min_size(
                pos - Vec2::new(6.0, 3.0),
                galley.size() + Vec2::new(12.0, 6.0),
            );
            painter.rect_filled(
                bg_rect,
                Rounding::same(5.0),
                Color32::from_rgba_unmultiplied(255, 255, 255, (alpha as f32 * 0.72) as u8),
            );
            painter.galley(pos, galley, c);
        }
    }
}

/// Rough.js-inspired double-pass slightly bowed sketchy line with progressive drawing.
fn draw_sketch_line(
    painter: &Painter,
    a: Pos2,
    b: Pos2,
    color: Color32,
    width: f32,
    progress: f32,
    rng: &mut Rng,
) {
    let dist = a.distance(b);
    if dist < 0.5 || progress <= 0.0 {
        return;
    }
    let dir = (b - a) / dist;
    let perp = Vec2::new(-dir.y, dir.x);
    let max_bow = (dist * 0.018).clamp(0.8, 3.8);

    for pass in 0..2 {
        let bow = rng.signed() * max_bow;
        let offset_a = Vec2::new(rng.signed() * 1.4, rng.signed() * 1.4);
        let offset_b = Vec2::new(rng.signed() * 1.4, rng.signed() * 1.4);
        let p0 = a + offset_a;
        let p2 = b + offset_b;
        let ctrl = p0.lerp(p2, 0.5) + perp * bow;

        let segs = 12usize;
        let max_t = progress.clamp(0.0, 1.0);
        let mut pts = Vec::with_capacity(segs + 1);
        for i in 0..=segs {
            let t = (i as f32 / segs as f32) * max_t;
            let inv = 1.0 - t;
            let x = inv * inv * p0.x + 2.0 * inv * t * ctrl.x + t * t * p2.x;
            let y = inv * inv * p0.y + 2.0 * inv * t * ctrl.y + t * t * p2.y;
            pts.push(Pos2::new(x, y));
        }
        let w = if pass == 0 { width } else { width * 0.65 };
        let c = if pass == 0 {
            color
        } else {
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 185)
        };
        painter.add(eframe::egui::Shape::line(pts, Stroke::new(w, c)));
    }
}

/// Smooth Catmull-Rom + pressure-tapered ink stroke (perfect-freehand aesthetic).
fn draw_tapered_stroke(
    painter: &Painter,
    pts: &[Pos2],
    color: Color32,
    base_width: f32,
    progress: f32,
) {
    if pts.len() < 2 || progress <= 0.0 {
        return;
    }
    // Subdivide with Catmull-Rom for smooth ink curves
    let mut smooth = Vec::with_capacity(pts.len() * 4);
    for i in 0..pts.len() - 1 {
        let p0 = if i == 0 { pts[0] } else { pts[i - 1] };
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = if i + 2 < pts.len() {
            pts[i + 2]
        } else {
            pts[pts.len() - 1]
        };
        let sub = 5;
        for s in 0..sub {
            let t = s as f32 / sub as f32;
            smooth.push(catmull_rom(p0, p1, p2, p3, t));
        }
    }
    smooth.push(*pts.last().unwrap());

    let visible_count = ((smooth.len() as f32) * progress.clamp(0.0, 1.0)).ceil() as usize;
    let visible_count = visible_count.clamp(2, smooth.len());

    let total = (smooth.len() - 1).max(1) as f32;
    for i in 0..(visible_count - 1) {
        let t = i as f32 / total;
        // Taper width smoothly at start and end of stroke
        let taper = (t * std::f32::consts::PI).sin().powf(0.45).clamp(0.35, 1.15);
        let w = (base_width * taper).max(1.0);
        painter.line_segment([smooth[i], smooth[i + 1]], Stroke::new(w, color));
    }
}

fn catmull_rom(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, t: f32) -> Pos2 {
    let t2 = t * t;
    let t3 = t2 * t;
    let x = 0.5
        * ((2.0 * p1.x)
            + (-p0.x + p2.x) * t
            + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
            + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3);
    let y = 0.5
        * ((2.0 * p1.y)
            + (-p0.y + p2.y) * t
            + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
            + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3);
    Pos2::new(x, y)
}

fn draw_badge(painter: &Painter, pos: Pos2, text: &str, accent: Color32, alpha: u8) {
    let fg = Color32::from_rgba_unmultiplied(255, 255, 255, alpha);
    let bg = Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha);
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(12.5), fg);
    let rect = Rect::from_min_size(
        Pos2::new(pos.x, pos.y - galley.size().y * 0.5),
        galley.size() + Vec2::new(14.0, 7.0),
    );
    painter.rect_filled(
        rect.translate(Vec2::new(0.0, 2.0)),
        Rounding::same(7.0),
        Color32::from_rgba_unmultiplied(0, 0, 0, (alpha as f32 * 0.16) as u8),
    );
    painter.rect_filled(rect, Rounding::same(7.0), bg);
    painter.galley(rect.min + Vec2::new(7.0, 3.5), galley, fg);
}

fn draw_centered_badge(painter: &Painter, center: Pos2, text: &str, accent: Color32, alpha: u8) {
    let fg = Color32::from_rgba_unmultiplied(255, 255, 255, alpha);
    let bg = Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha);
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(12.0), fg);
    let rect = Rect::from_center_size(center, galley.size() + Vec2::new(14.0, 6.0));
    painter.rect_filled(
        rect.translate(Vec2::new(0.0, 2.0)),
        Rounding::same(7.0),
        Color32::from_rgba_unmultiplied(0, 0, 0, (alpha as f32 * 0.16) as u8),
    );
    painter.rect_filled(rect, Rounding::same(7.0), bg);
    painter.galley(rect.min + Vec2::new(7.0, 3.0), galley, fg);
}

fn draw_pointer(painter: &Painter, viewport: Rect, ptr: &PointerState) {
    let pos = to_screen(viewport, ptr.x, ptr.y);
    let pulse_t = (ptr.pulse.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
    let ring_r = 10.0 + pulse_t * 14.0;
    let ring_alpha = ((1.0 - pulse_t) * 160.0) as u8;

    // Pulsing target ring
    painter.circle_stroke(
        pos,
        ring_r,
        Stroke::new(
            2.2_f32,
            Color32::from_rgba_unmultiplied(ptr.color.r(), ptr.color.g(), ptr.color.b(), ring_alpha),
        ),
    );

    // Cursor triangle
    let tip = pos;
    let p1 = pos + Vec2::new(4.0, 20.0);
    let p2 = pos + Vec2::new(10.0, 14.0);
    let p3 = pos + Vec2::new(18.0, 14.0);
    painter.add(eframe::egui::Shape::convex_polygon(
        vec![tip, p1, p2, p3],
        ptr.color,
        Stroke::new(1.8_f32, Color32::WHITE),
    ));

    // Floating label pill
    if let Some(label) = &ptr.label {
        let badge_pos = pos + Vec2::new(22.0, 16.0);
        let galley = painter.layout_no_wrap(
            label.clone(),
            FontId::proportional(13.5),
            Color32::WHITE,
        );
        let rect = Rect::from_min_size(badge_pos, galley.size() + Vec2::new(14.0, 8.0));
        painter.rect_filled(rect, Rounding::same(8.0), ptr.color);
        painter.rect_stroke(rect, Rounding::same(8.0), Stroke::new(1.2_f32, Color32::WHITE));
        painter.galley(rect.min + Vec2::new(7.0, 4.0), galley, Color32::WHITE);
    }

    // Tiny crosshair dot at tip
    painter.circle_filled(pos, 2.5, Color32::WHITE);
    let _ = Align2::LEFT_TOP;
}
