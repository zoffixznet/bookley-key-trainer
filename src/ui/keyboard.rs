//! The on-screen keyboard widget: a real full-size board drawn as physical keycaps.
//! Guide mode rings the next key (and the Shift keys when the target needs Shift) in
//! verdigris; Feedback mode flashes just-pressed keys with a fading brass ink-stamp;
//! Hidden mode draws nothing.
//!
//! Caps read as physical keys: neutral face, subtle top highlight, a darker bottom lip,
//! and the finger zone shown as a small accent bar on the lip (not a full-face tint).

use std::collections::HashMap;

use egui::{Align2, Color32, CornerRadius, FontId, Key, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::core::config::KeyboardMode;
use crate::core::keys::{self, CapId, Slot};
use crate::ui::theme::Palette;

/// Tracks recently-pressed keys for the Feedback-mode flash. Each entry stores the time it
/// was pressed; the widget animates a fade from there.
#[derive(Default)]
pub struct FlashState {
    pressed_at: HashMap<CapId, f64>,
}

impl FlashState {
    pub fn press(&mut self, key: Key, now: f64) {
        self.pressed_at.insert(CapId::K(key), now);
    }
    /// Flash both Shift caps (egui reports the modifier, not which side).
    pub fn press_shift(&mut self, now: f64) {
        self.pressed_at.insert(CapId::ShiftL, now);
        self.pressed_at.insert(CapId::ShiftR, now);
    }
    /// Drop entries older than the fade window to keep the map small.
    pub fn prune(&mut self, now: f64, fade: f64) {
        self.pressed_at.retain(|_, t| now - *t < fade);
    }
    fn intensity(&self, id: CapId, now: f64, fade: f64) -> f32 {
        match self.pressed_at.get(&id) {
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

/// Draw the keyboard. `next_key` is the physical key to ring in Guide mode; when
/// `needs_shift` is set, the Shift caps are ringed with it. Returns the height used.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    ui: &mut egui::Ui,
    palette: &Palette,
    mode: KeyboardMode,
    next_key: Option<Key>,
    needs_shift: bool,
    flash: &FlashState,
    reduced_motion: bool,
    now: f64,
    show_numpad: bool,
) -> f32 {
    if mode == KeyboardMode::Hidden {
        return 0.0;
    }

    let board = keys::board(show_numpad);
    let avail_w = ui.available_width();
    let gap = 5.0;
    // Unit size: fill the available width up to a comfortable cap size.
    let unit = ((avail_w - gap) / board.units_wide - gap).clamp(18.0, 52.0);
    let key_h = unit.clamp(20.0, 48.0);
    let row_h = key_h + gap;
    // The F-row gets a little extra breathing room below it, like a real board.
    let f_row_extra = gap * 1.5;
    let total_h = row_h * board.rows.len() as f32 + f_row_extra + gap;
    let board_w = board.units_wide * (unit + gap) + gap;

    let (rect, _resp) = ui.allocate_exact_size(Vec2::new(avail_w, total_h), Sense::hover());
    let painter = ui.painter_at(rect.expand(4.0));
    let left = rect.left() + (avail_w - board_w).max(0.0) / 2.0;

    let mut y = rect.top();
    for (ri, row) in board.rows.iter().enumerate() {
        let mut x = left + gap;
        for slot in row {
            match slot {
                Slot::Gap(g) => {
                    x += g * (unit + gap);
                }
                Slot::Cap(cap) => {
                    let w = cap.width * (unit + gap) - gap;
                    let h = if cap.height > 1.5 {
                        key_h * 2.0 + gap
                    } else {
                        key_h
                    };
                    let kr = Rect::from_min_size(egui::pos2(x, y), Vec2::new(w, h));
                    draw_cap(
                        &painter,
                        palette,
                        cap,
                        kr,
                        mode,
                        next_key,
                        needs_shift,
                        flash,
                        reduced_motion,
                        now,
                    );
                    x += cap.width * (unit + gap);
                }
            }
        }
        y += row_h;
        if ri == 0 {
            y += f_row_extra;
        }
    }
    total_h
}

#[allow(clippy::too_many_arguments)]
fn draw_cap(
    painter: &egui::Painter,
    p: &Palette,
    cap: &keys::KeyCap,
    rect: Rect,
    mode: KeyboardMode,
    next_key: Option<Key>,
    needs_shift: bool,
    flash: &FlashState,
    reduced_motion: bool,
    now: f64,
) {
    let radius = CornerRadius::same(5);
    let is_next = mode == KeyboardMode::Guide
        && (next_key.map(CapId::K) == Some(cap.id)
            || (needs_shift && (cap.id == CapId::ShiftL || cap.id == CapId::ShiftR)));

    // Drop shadow / bottom lip: a darker rect nudged down, so the cap reads as raised.
    let lip = Rect::from_min_max(
        rect.min + Vec2::new(0.0, 2.0),
        rect.max + Vec2::new(0.0, 2.5),
    );
    painter.rect_filled(lip, radius, p.key_lip);

    // Cap face.
    let mut face = p.key_face;
    // Feedback flash: fading brass ink-stamp on just-pressed keys.
    if mode == KeyboardMode::Feedback {
        let i = if reduced_motion {
            if flash.intensity(cap.id, now, 0.15) > 0.0 {
                0.85
            } else {
                0.0
            }
        } else {
            flash.intensity(cap.id, now, FADE_SECS)
        };
        if i > 0.0 {
            face = lerp_color(face, p.brass, i * 0.85);
        }
    }
    if is_next {
        face = lerp_color(face, p.verdigris, 0.22);
    }
    painter.rect_filled(rect, radius, face);

    // Subtle top highlight so the cap reads dished/convex.
    let top = Rect::from_min_max(
        rect.min + Vec2::new(1.5, 1.5),
        egui::pos2(rect.max.x - 1.5, rect.min.y + rect.height() * 0.45),
    );
    painter.rect_filled(top, CornerRadius::same(4), p.key_top);

    // Finger-zone accent: a thin bar sitting on the cap's lower lip.
    let zone = p.finger[cap.finger.zone()];
    let bar_w = (rect.width() * 0.44).clamp(6.0, 26.0);
    let bar = Rect::from_center_size(
        egui::pos2(rect.center().x, rect.max.y - 3.5),
        Vec2::new(bar_w, 2.5),
    );
    painter.rect_filled(bar, CornerRadius::same(1), zone);

    // Edge stroke; the guide ring is stronger and verdigris.
    let (stroke_w, stroke_c) = if is_next {
        (2.0, p.verdigris)
    } else {
        (1.0, p.key_edge)
    };
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(stroke_w, stroke_c),
        StrokeKind::Inside,
    );

    // Label.
    let fs = if cap.label.chars().count() > 2 {
        (rect.height() * 0.30).clamp(8.0, 12.0)
    } else {
        (rect.height() * 0.40).clamp(10.0, 16.0)
    };
    let label_color = if is_next { p.verdigris } else { p.key_text };
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        cap.label,
        FontId::monospace(fs),
        label_color,
    );

    // Home-row bumps on F and J.
    if cap.id == CapId::K(Key::F) || cap.id == CapId::K(Key::J) {
        let bump_y = rect.max.y - 7.0;
        painter.line_segment(
            [
                egui::pos2(rect.center().x - 4.0, bump_y),
                egui::pos2(rect.center().x + 4.0, bump_y),
            ],
            Stroke::new(2.0, p.key_text.linear_multiply(0.65)),
        );
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}
