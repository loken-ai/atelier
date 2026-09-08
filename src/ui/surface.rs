//! Painted surfaces. Everything here paints into a rect the caller already
//! owns and allocates nothing, except `meter`, which is a cell. Every surface
//! reads the palette, so both skins get their own hairline weights without a
//! branch: the light source is above in both, the well is always the darkest
//! surface and the plate the lightest.

use eframe::egui::{
    self, epaint, Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, StrokeKind, Ui, Vec2,
};

use crate::theme::{palette, text, HAIRLINE};

/// Corner radius of a plate.
pub const PLATE_RADIUS: f32 = 4.0;
/// Corner radius of a well.
pub const WELL_RADIUS: f32 = 3.0;
/// Horizontal inset of the lip shadow, so it stops inside the rounded corner.
const LIP_INSET: f32 = 2.0;
/// Width of the shadow under a lip.
const LIP_W: f32 = 1.5;
/// Half a pixel: a one-pixel line is drawn on the pixel centre.
const HALF_PX: f32 = 0.5;
/// Width of the bezel shadow at the top of a screen.
const SCREEN_BEZEL_W: f32 = 2.0;
/// Distance from the top of a heading to its rule.
const HEADING_RULE_DY: f32 = 8.0;
/// Strength of a heading's rule relative to its ink.
const HEADING_RULE_GAMMA: f32 = 0.35;
/// Outer halo of a lit lamp, as a multiple of its radius, and its strength.
const LAMP_HALO_OUTER_R: f32 = 2.2;
const LAMP_HALO_OUTER_GAMMA: f32 = 0.20;
/// Inner halo of a lit lamp.
const LAMP_HALO_INNER_R: f32 = 1.5;
const LAMP_HALO_INNER_GAMMA: f32 = 0.35;
/// Offset and radius of the glint on a lit lamp, as multiples of its radius.
const GLINT_OFFSET: f32 = 0.35;
const GLINT_R: f32 = 0.28;
/// Corner radius of a meter's well.
const METER_RADIUS: f32 = 2.0;
/// The fill of a meter catches less light than a lamp.
const METER_CATCH_DIV: u8 = 4;
/// Inset of a chevron's points from its rect, as a fraction of its width.
const CHEVRON_INSET: f32 = 0.3;

fn black(alpha: u8) -> Color32 {
    Color32::from_black_alpha(alpha)
}

fn white(alpha: u8) -> Color32 {
    Color32::from_white_alpha(alpha)
}

/// A vertical gradient as one mesh: four vertices, two triangles.
pub fn gradient_shape(rect: Rect, top: Color32, bottom: Color32) -> Shape {
    let mut mesh = epaint::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    Shape::mesh(mesh)
}

pub fn gradient_v(ui: &Ui, rect: Rect, top: Color32, bottom: Color32) {
    ui.painter().add(gradient_shape(rect, top, bottom));
}

/// A raised plate: lighter at the top, an edge, a shadow under the lip and a
/// catch of light on the bottom.
pub fn plate_shape(rect: Rect, radius: f32) -> Shape {
    let p = palette();
    let lip_y = rect.top() + HALF_PX + HAIRLINE;
    let catch_y = rect.bottom() - HALF_PX - HAIRLINE;
    Shape::Vec(vec![
        gradient_shape(rect, p.plate_top, p.plate_bottom),
        Shape::rect_stroke(rect, radius, Stroke::new(HAIRLINE, p.plate_edge), StrokeKind::Inside),
        Shape::line_segment(
            [Pos2::new(rect.left() + LIP_INSET, lip_y), Pos2::new(rect.right() - LIP_INSET, lip_y)],
            Stroke::new(LIP_W, black(p.lip_shadow_a)),
        ),
        Shape::line_segment(
            [Pos2::new(rect.left() + LIP_INSET, catch_y), Pos2::new(rect.right() - LIP_INSET, catch_y)],
            Stroke::new(HAIRLINE, white(p.light_catch_a)),
        ),
    ])
}

pub fn plate(ui: &Ui, rect: Rect, radius: f32) {
    ui.painter().add(plate_shape(rect, radius));
}

/// A sunk well: the near wall is in shadow, the far wall catches the light.
pub fn recess_shape(rect: Rect, radius: f32) -> Shape {
    let p = palette();
    let shadow_y = rect.top() + HALF_PX;
    let catch_y = rect.bottom() - HALF_PX;
    Shape::Vec(vec![
        Shape::rect_filled(rect, radius, p.well),
        Shape::line_segment(
            [Pos2::new(rect.left() + radius, shadow_y), Pos2::new(rect.right() - radius, shadow_y)],
            Stroke::new(LIP_W, black(p.well_shadow_a)),
        ),
        Shape::line_segment(
            [Pos2::new(rect.left() + radius, catch_y), Pos2::new(rect.right() - radius, catch_y)],
            Stroke::new(HAIRLINE, white(p.well_catch_a)),
        ),
        Shape::rect_stroke(rect, radius, Stroke::new(HAIRLINE, p.well_edge), StrokeKind::Inside),
    ])
}

pub fn recess(ui: &Ui, rect: Rect, radius: f32) {
    ui.painter().add(recess_shape(rect, radius));
}

/// A screen: a well with the glass over it, so only a bezel shadow at the top.
pub fn screen_shape(rect: Rect) -> Shape {
    let p = palette();
    let bezel_y = rect.top() + SCREEN_BEZEL_W / 2.0;
    Shape::Vec(vec![
        Shape::rect_filled(rect, WELL_RADIUS, p.well),
        Shape::line_segment(
            [Pos2::new(rect.left() + WELL_RADIUS, bezel_y), Pos2::new(rect.right() - WELL_RADIUS, bezel_y)],
            Stroke::new(SCREEN_BEZEL_W, black(p.well_shadow_a)),
        ),
    ])
}

pub fn screen(ui: &Ui, rect: Rect) {
    ui.painter().add(screen_shape(rect));
}

/// Text screened onto a surface: a pale copy one pixel below, then the ink.
pub fn printed(ui: &Ui, pos: Pos2, s: &str, font: FontId, ink: Color32, anchor: Align2) {
    let p = palette();
    let painter = ui.painter();
    painter.text(pos + Vec2::new(0.0, 1.0), anchor, s, font.clone(), white(p.print_halo_a));
    painter.text(pos, anchor, s, font, ink);
}

/// Letter-spaced text, painted one glyph at a time from `left`. Returns the
/// width painted.
pub fn tracked_text(ui: &Ui, left: Pos2, s: &str, font: FontId, ink: Color32, tracking: f32) -> f32 {
    let mut x = left.x;
    let mut buf = [0u8; 4];
    for ch in s.chars() {
        let glyph = ch.encode_utf8(&mut buf);
        let w = ui.fonts_mut(|f| f.layout_no_wrap(glyph.to_owned(), font.clone(), ink).size().x);
        printed(ui, Pos2::new(x, left.y), glyph, font.clone(), ink, Align2::LEFT_TOP);
        x += w + tracking;
    }
    (x - left.x - tracking).max(0.0)
}

/// A heading: tracked capitals over a hairline that runs to `rule_to`.
pub fn heading(ui: &Ui, at: Pos2, caps: &str, rule_to: f32) {
    let p = palette();
    let font = FontId::proportional(text::HEADING_PT);
    let w = tracked_text(ui, at, caps, font, p.ink_dim, text::TRACKING);
    let y = at.y + HEADING_RULE_DY;
    ui.painter().line_segment(
        [Pos2::new(at.x, y), Pos2::new(rule_to.max(at.x + w), y)],
        Stroke::new(HAIRLINE, p.ink_dim.gamma_multiply(HEADING_RULE_GAMMA)),
    );
}

/// Prose under a control, one painted line per input line at a fixed pitch.
/// Returns the bottom.
pub fn caption(ui: &Ui, at: Pos2, s: &str) -> f32 {
    let p = palette();
    let font = FontId::proportional(text::CAPTION_PT);
    let mut y = at.y;
    for line in s.lines() {
        ui.painter().text(Pos2::new(at.x, y), Align2::LEFT_TOP, line, font.clone(), p.ink_dim);
        y += text::CAPTION_PITCH;
    }
    y
}

/// A lamp: a halo and a glint when lit, the off body otherwise.
pub fn lamp(ui: &Ui, centre: Pos2, r: f32, on: bool, tint: Color32) {
    let p = palette();
    let painter = ui.painter();
    if on {
        painter.circle_filled(centre, r * LAMP_HALO_OUTER_R, tint.gamma_multiply(LAMP_HALO_OUTER_GAMMA));
        painter.circle_filled(centre, r * LAMP_HALO_INNER_R, tint.gamma_multiply(LAMP_HALO_INNER_GAMMA));
    }
    painter.circle_filled(centre, r, if on { tint } else { p.lamp_off });
    painter.circle_stroke(centre, r, Stroke::new(HAIRLINE, p.border));
    if on {
        painter.circle_filled(centre - Vec2::splat(r * GLINT_OFFSET), r * GLINT_R, white(p.glint_a));
    }
}

/// A meter: a sunk well of `size` with `fraction` of it filled in `tint`.
/// The one surface that allocates, because a meter is a cell in a row.
pub fn meter(ui: &mut Ui, fraction: f32, size: Vec2, tint: Color32) -> egui::Response {
    let p = palette();
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    recess(ui, rect, METER_RADIUS);
    let fill_w = fraction.clamp(0.0, 1.0) * rect.width();
    if fill_w > 0.0 {
        let fill = Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
        let painter = ui.painter();
        painter.rect_filled(fill, METER_RADIUS, tint);
        let y = fill.top() + HALF_PX;
        painter.line_segment(
            [Pos2::new(fill.left(), y), Pos2::new(fill.right(), y)],
            Stroke::new(HAIRLINE, white(p.glint_a / METER_CATCH_DIV)),
        );
    }
    response
}

/// Which way a chevron points.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Chevron {
    Left,
    Right,
}

/// A chevron drawn with two strokes, pointing `dir`, inset from `rect`.
pub fn chevron(ui: &Ui, rect: Rect, dir: Chevron, stroke: Stroke) {
    let inset = rect.width() * CHEVRON_INSET;
    let (near, far) = match dir {
        Chevron::Left => (rect.right() - inset, rect.left() + inset),
        Chevron::Right => (rect.left() + inset, rect.right() - inset),
    };
    let mid = rect.center().y;
    let painter = ui.painter();
    painter.line_segment([Pos2::new(near, rect.top() + inset), Pos2::new(far, mid)], stroke);
    painter.line_segment([Pos2::new(far, mid), Pos2::new(near, rect.bottom() - inset)], stroke);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;

    /// Runs `body` once in a headless frame on the skin asked for.
    fn frame(dark: bool, body: impl FnMut(&mut Ui)) {
        let mut harness = egui_kittest::Harness::new_ui(body);
        theme::apply(&harness.ctx, dark);
        harness.run();
    }

    /// Every surface paints on a normal, a zero-size and a one-pixel rect in
    /// both skins without panicking.
    #[test]
    fn the_surfaces_paint() {
        for dark in [true, false] {
            frame(dark, |ui| {
                for rect in [
                    Rect::from_min_size(Pos2::new(4.0, 4.0), Vec2::new(200.0, 60.0)),
                    Rect::from_min_size(Pos2::new(4.0, 4.0), Vec2::ZERO),
                    Rect::from_min_size(Pos2::new(4.0, 4.0), Vec2::splat(1.0)),
                ] {
                    plate(ui, rect, PLATE_RADIUS);
                    recess(ui, rect, WELL_RADIUS);
                    screen(ui, rect);
                    let font = FontId::proportional(text::HEADING_PT);
                    printed(ui, rect.left_top(), "PRINTED", font.clone(), palette().ink, Align2::LEFT_TOP);
                    tracked_text(ui, rect.left_top(), "", font.clone(), palette().ink, text::TRACKING);
                    tracked_text(ui, rect.left_top(), "AB", font, palette().ink, text::TRACKING);
                    heading(ui, rect.left_top(), "HEADING", rect.right());
                    caption(ui, rect.left_top(), "one\ntwo");
                    caption(ui, rect.left_top(), "");
                    lamp(ui, rect.center(), 4.0, true, palette().accent);
                    lamp(ui, rect.center(), 4.0, false, palette().accent);
                    chevron(ui, rect, Chevron::Left, Stroke::new(HAIRLINE, palette().ink));
                    chevron(ui, rect, Chevron::Right, Stroke::new(HAIRLINE, palette().ink));
                }
                for fraction in [-1.0, 0.0, 0.5, 2.0] {
                    meter(ui, fraction, Vec2::new(60.0, 8.0), palette().accent);
                }
            });
        }
    }

    #[test]
    fn tracked_text_is_wider_than_its_letters() {
        frame(true, |ui| {
            let font = FontId::proportional(text::HEADING_PT);
            let glyphs: f32 = ["A", "B"]
                .iter()
                .map(|g| ui.fonts_mut(|f| f.layout_no_wrap(g.to_string(), font.clone(), Color32::WHITE).size().x))
                .sum();
            let tracked = tracked_text(ui, Pos2::ZERO, "AB", font, Color32::WHITE, text::TRACKING);
            assert!(tracked > glyphs, "tracked {tracked} must exceed the bare glyphs {glyphs}");
        });
    }

    #[test]
    fn a_meter_is_a_cell_of_the_size_asked() {
        frame(true, |ui| {
            let size = Vec2::new(60.0, 8.0);
            let response = meter(ui, 0.5, size, palette().accent);
            assert_eq!(response.rect.size(), size);
        });
    }
}
