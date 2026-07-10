//! The on-screen letterpress keyboard widget. Highlights the next key by physical position
//! (Guide mode) with a steady verdigris ring, and flashes just-pressed physical keys
//! (Feedback mode) with a fading brass ink-stamp. Hidden mode draws nothing.

use std::collections::HashMap;

use egui::{Align2, Color32, CornerRadius, FontId, Key, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::core::config::KeyboardMode;
use crate::core::keys;
use crate::ui::theme::Palette;

/// Tracks recently-pressed keys for the Feedback-mode flash. Each entry stores the time it
/// was pressed; the widget animates a fade from there.
#[derive(Default)]
pub struct FlashState {
    pressed_at: HashMap<Key, f64>,
}

impl FlashState {
    pub fn press(&mut self, key: Key, now: f64) {
        self.pressed_at.insert(key, now);
    }
    /// Drop entries older than the fade window to keep the map small.
    pub fn prune(&mut self, now: f64, fade: f64) {
        self.pressed_at.retain(|_, t| now - *t < fade);
    }
    fn intensity(&self, key: Key, now: f64, fade: f64) -> f32 {
        match self.pressed_at.get(&key) {
            Some(t) => {
                let age = (now - *t).max(0.0);
                (1.0 - (age / fade)).clamp(0.0, 1.0) as f32
            }
            None => 0.0,
        }
    }
    pub fn is_animating(&self, now: f64, fade: f64) -> bool {
        self.pressed_at.values().any(|t| now - *t < fade)
    }
}

const FADE_SECS: f64 = 0.45;

/// Draw the keyboard. `next_key` is the physical key to ring in Guide mode.
/// Returns the total height used (0 in Hidden mode).
#[allow(clippy::too_many_arguments)]
pub fn draw(
    ui: &mut egui::Ui,
    palette: &Palette,
    mode: KeyboardMode,
    next_key: Option<Key>,
    flash: &FlashState,
    reduced_motion: bool,
    now: f64,
) -> f32 {
    if mode == KeyboardMode::Hidden {
        return 0.0;
    }

    let layout = keys::layout();
    let avail_w = ui.available_width().min(1100.0);
    // Compute unit size from the widest row.
    let max_units: f32 = layout
        .iter()
        .map(|row| row.iter().map(|c| c.width).sum::<f32>())
        .fold(0.0, f32::max);
    let gap = 4.0;
    let n_max_keys = layout
        .iter()
        .map(|r| r.len())
        .max()
        .unwrap_or(1) as f32;
    let unit = ((avail_w - gap * (n_max_keys + 1.0)) / max_units).clamp(20.0, 46.0);
    let key_h = (unit * 1.05).clamp(22.0, 44.0);
    let row_h = key_h + gap;
    let total_h = row_h * layout.len() as f32 + gap;

    let (rect, _resp) =
        ui.allocate_exact_size(Vec2::new(avail_w, total_h), Sense::hover());
    let painter = ui.painter_at(rect);

    let mut y = rect.top() + gap;
    for row in &layout {
        let row_units: f32 = row.iter().map(|c| c.width).sum();
        let row_w = row_units * unit + gap * (row.len() as f32 + 1.0);
        let mut x = rect.left() + (avail_w - row_w).max(0.0) / 2.0 + gap;
        for cap in row {
            let w = cap.width * unit;
            let kr = Rect::from_min_size(egui::pos2(x, y), Vec2::new(w, key_h));
            draw_cap(&painter, palette, cap, kr, mode, next_key, flash, reduced_motion, now);
            x += w + gap;
        }
        y += row_h;
    }
    total_h
}

#[allow(clippy::too_many_arguments)]
fn draw_cap(
    painter: &egui::Painter,
    palette: &Palette,
    cap: &keys::KeyCap,
    rect: Rect,
    mode: KeyboardMode,
    next_key: Option<Key>,
    flash: &FlashState,
    reduced_motion: bool,
    now: f64,
) {
    let radius = CornerRadius::same(5);
    let zone = cap.finger.zone();
    let base = palette.finger[zone];

    // Keycap body with a slight top-highlight to read as a mechanical cap.
    painter.rect_filled(rect, radius, base);
    let top = Rect::from_min_max(
        rect.min,
        egui::pos2(rect.max.x, rect.min.y + rect.height() * 0.42),
    );
    painter.rect_filled(top, radius, base.linear_multiply(1.18));

    // Feedback flash: fading brass ink-stamp on just-pressed keys.
    if mode == KeyboardMode::Feedback && !reduced_motion {
        let i = flash.intensity(cap.key, now, FADE_SECS);
        if i > 0.0 {
            let stamp = lerp_color(base, palette.brass, i * 0.9);
            painter.rect_filled(rect, radius, stamp);
        }
    } else if mode == KeyboardMode::Feedback && reduced_motion {
        // Reduced motion: a static brass tint if pressed very recently, no animation.
        if flash.intensity(cap.key, now, 0.12) > 0.0 {
            painter.rect_filled(rect, radius, palette.brass.linear_multiply(0.7));
        }
    }

    // Guide next-key ring.
    let is_next = mode == KeyboardMode::Guide && next_key == Some(cap.key);
    let (stroke_w, stroke_c) = if is_next {
        (2.5, palette.verdigris)
    } else {
        (1.0, palette.ghost.linear_multiply(0.5))
    };
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(stroke_w, stroke_c),
        StrokeKind::Inside,
    );

    // Label.
    let fs = (rect.height() * 0.42).clamp(9.0, 16.0);
    let label_color = if is_next { palette.verdigris } else { palette.paper };
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        cap.label,
        FontId::monospace(fs),
        label_color,
    );

    // Home-row bumps on F and J.
    if cap.key == Key::F || cap.key == Key::J {
        let bump_y = rect.max.y - 4.0;
        painter.line_segment(
            [
                egui::pos2(rect.center().x - 5.0, bump_y),
                egui::pos2(rect.center().x + 5.0, bump_y),
            ],
            Stroke::new(1.5, palette.paper.linear_multiply(0.7)),
        );
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
    )
}
