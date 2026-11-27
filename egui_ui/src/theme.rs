//! Grimlite Ri Design System — reusable modern dark theme & styled UI components (SSOT).
//!
//! Provides clear visual hierarchy between:
//! - Sunken input fields (`text_input`, `num_input`) — darkest inset fill + subtle border
//! - Dropdown selectors (`dropdown`) — mid-dark surface + distinct border
//! - Action buttons (`action_button`, `primary_button`, `danger_button`, `toggle_button`) — elevated fill + crisp border
//! - Section cards (`card`) — elevated slate panel with subtle border

use egui::emath::Numeric;
use egui::{Color32, ComboBox, Context, CornerRadius, DragValue, Margin, Response, RichText, Stroke, TextEdit, Ui, Visuals, WidgetText};
use std::hash::Hash;
use std::ops::RangeInclusive;

/// Centralized color tokens for Grimlite Ri (Desktop + Headless).
pub struct Palette;

impl Palette {
    pub const BG_BASE: Color32 = Color32::from_rgb(19, 22, 28);
    pub const BG_SURFACE: Color32 = Color32::from_rgb(25, 29, 38);
    pub const BG_INPUT: Color32 = Color32::from_rgb(13, 15, 20);
    pub const BG_DROPDOWN: Color32 = Color32::from_rgb(29, 34, 44);
    pub const BG_BUTTON: Color32 = Color32::from_rgb(42, 49, 63);
    pub const BG_BUTTON_HOVER: Color32 = Color32::from_rgb(54, 63, 82);
    pub const BG_BUTTON_ACTIVE: Color32 = Color32::from_rgb(31, 36, 48);

    pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(40, 46, 58);
    pub const BORDER_CONTROL: Color32 = Color32::from_rgb(60, 70, 90);
    pub const BORDER_HOVER: Color32 = Color32::from_rgb(86, 100, 128);
    pub const BORDER_FOCUS: Color32 = Color32::from_rgb(56, 189, 248);

    pub const ACCENT_BRAND: Color32 = Color32::from_rgb(230, 57, 70);
    pub const ACCENT_PRIMARY: Color32 = Color32::from_rgb(18, 86, 134);
    pub const ACCENT_PRIMARY_HOVER: Color32 = Color32::from_rgb(24, 108, 166);
    pub const ACCENT_SELECT: Color32 = Color32::from_rgb(16, 76, 114);

    pub const SUCCESS: Color32 = Color32::from_rgb(74, 222, 128);
    pub const SUCCESS_BG: Color32 = Color32::from_rgb(20, 62, 42);
    pub const WARNING: Color32 = Color32::from_rgb(251, 191, 36);
    pub const DANGER: Color32 = Color32::from_rgb(248, 113, 113);
    pub const DANGER_BG: Color32 = Color32::from_rgb(68, 28, 34);

    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(230, 234, 242);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(142, 153, 173);
    pub const TEXT_TITLE: Color32 = Color32::from_rgb(168, 182, 204);
}

/// Applies the Grimlite Ri modern dark theme globally to an `egui::Context`.
pub fn apply(ctx: &Context) {
    let mut visuals = Visuals::dark();

    visuals.panel_fill = Palette::BG_BASE;
    visuals.window_fill = Palette::BG_BASE;
    visuals.faint_bg_color = Palette::BG_SURFACE;
    visuals.extreme_bg_color = Palette::BG_INPUT;
    visuals.code_bg_color = Palette::BG_INPUT;

    visuals.window_corner_radius = CornerRadius::same(6);
    visuals.window_stroke = Stroke::new(1.0, Palette::BORDER_SUBTLE);
    visuals.menu_corner_radius = CornerRadius::same(5);

    visuals.selection.bg_fill = Palette::ACCENT_SELECT;
    visuals.selection.stroke = Stroke::new(1.0, Palette::BORDER_FOCUS);

    // Non-interactive surfaces (labels, separators, frames)
    visuals.widgets.noninteractive.bg_fill = Palette::BG_SURFACE;
    visuals.widgets.noninteractive.weak_bg_fill = Palette::BG_SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Palette::BORDER_SUBTLE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Palette::TEXT_PRIMARY);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(4);

    // Inactive buttons / controls (elevated slate + crisp border)
    visuals.widgets.inactive.bg_fill = Palette::BG_BUTTON;
    visuals.widgets.inactive.weak_bg_fill = Palette::BG_BUTTON;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Palette::BORDER_CONTROL);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Palette::TEXT_PRIMARY);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(4);

    // Hovered buttons / controls
    visuals.widgets.hovered.bg_fill = Palette::BG_BUTTON_HOVER;
    visuals.widgets.hovered.weak_bg_fill = Palette::BG_BUTTON_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Palette::BORDER_HOVER);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(4);
    visuals.widgets.hovered.expansion = 0.0;

    // Active / pressed
    visuals.widgets.active.bg_fill = Palette::BG_BUTTON_ACTIVE;
    visuals.widgets.active.weak_bg_fill = Palette::BG_BUTTON_ACTIVE;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, Palette::BORDER_FOCUS);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::same(4);
    visuals.widgets.active.expansion = 0.0;

    // Open dropdown / popup trigger
    visuals.widgets.open.bg_fill = Palette::BG_DROPDOWN;
    visuals.widgets.open.weak_bg_fill = Palette::BG_DROPDOWN;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, Palette::BORDER_FOCUS);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.open.corner_radius = CornerRadius::same(4);

    ctx.set_visuals(visuals);
}

/// Framed elevated card container with a subtle muted title.
pub fn card(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(Palette::BG_SURFACE)
        .stroke(Stroke::new(1.0, Palette::BORDER_SUBTLE))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if !title.is_empty() {
                ui.label(
                    RichText::new(title)
                        .size(11.0)
                        .strong()
                        .color(Palette::TEXT_TITLE),
                );
                ui.add_space(3.0);
            }
            add_contents(ui);
        });
}

/// Sunken single-line text field with clear inset background and focus border.
pub fn text_input(ui: &mut Ui, value: &mut String, hint: &str, width: f32) -> Response {
    let mut edit = TextEdit::singleline(value)
        .background_color(Palette::BG_INPUT)
        .desired_width(width);
    if !hint.is_empty() {
        edit = edit.hint_text(RichText::new(hint).color(Palette::TEXT_MUTED));
    }
    ui.add(edit)
}

/// Sunken numeric input (`DragValue` styled as an inset input field rather than a gray button).
pub fn num_input<N: Numeric>(
    ui: &mut Ui,
    value: &mut N,
    range: RangeInclusive<N>,
    width: f32,
) -> Response {
    ui.scope(|ui| {
        let w = &mut ui.style_mut().visuals.widgets;
        w.inactive.weak_bg_fill = Palette::BG_INPUT;
        w.inactive.bg_fill = Palette::BG_INPUT;
        w.inactive.bg_stroke = Stroke::new(1.0, Palette::BORDER_CONTROL);
        w.hovered.weak_bg_fill = Palette::BG_INPUT;
        w.hovered.bg_fill = Palette::BG_INPUT;
        w.hovered.bg_stroke = Stroke::new(1.0, Palette::BORDER_HOVER);
        w.active.weak_bg_fill = Palette::BG_INPUT;
        w.active.bg_fill = Palette::BG_INPUT;
        w.active.bg_stroke = Stroke::new(1.0, Palette::BORDER_FOCUS);

        ui.add_sized(
            [width, 20.0],
            DragValue::new(value).range(range),
        )
    })
    .inner
}

/// Distinct mid-tone dropdown (`ComboBox`) visually separated from both inputs and buttons.
pub fn dropdown<R>(
    ui: &mut Ui,
    id_salt: impl Hash + std::fmt::Debug,
    selected_text: impl Into<WidgetText>,
    width: f32,
    menu_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    ui.scope(|ui| {
        let w = &mut ui.style_mut().visuals.widgets;
        w.inactive.weak_bg_fill = Palette::BG_DROPDOWN;
        w.inactive.bg_fill = Palette::BG_DROPDOWN;
        w.inactive.bg_stroke = Stroke::new(1.0, Palette::BORDER_CONTROL);
        w.hovered.weak_bg_fill = Palette::BG_BUTTON;
        w.hovered.bg_stroke = Stroke::new(1.0, Palette::BORDER_HOVER);

        ComboBox::from_id_salt(id_salt)
            .selected_text(selected_text)
            .width(width)
            .show_ui(ui, menu_contents)
    })
    .inner
}

/// Highlighted primary action button (explicit size).
pub fn primary_button(ui: &mut Ui, label: &str, size: [f32; 2]) -> Response {
    primary_button_enabled(ui, true, label, size)
}

/// Highlighted primary action button (explicit size + enabled flag).
pub fn primary_button_enabled(
    ui: &mut Ui,
    enabled: bool,
    label: &str,
    size: [f32; 2],
) -> Response {
    ui.scope(|ui| {
        if enabled {
            let w = &mut ui.style_mut().visuals.widgets;
            w.inactive.weak_bg_fill = Palette::ACCENT_PRIMARY;
            w.inactive.bg_fill = Palette::ACCENT_PRIMARY;
            w.inactive.bg_stroke = Stroke::new(1.0, Palette::BORDER_FOCUS);
            w.hovered.weak_bg_fill = Palette::ACCENT_PRIMARY_HOVER;
            w.hovered.bg_fill = Palette::ACCENT_PRIMARY_HOVER;
            w.hovered.bg_stroke = Stroke::new(1.0, Palette::BORDER_FOCUS);
        }
        let text = if enabled {
            RichText::new(label).strong().color(Color32::WHITE)
        } else {
            RichText::new(label)
        };
        ui.add_enabled(enabled, egui::Button::new(text).min_size(size.into()))
    })
    .inner
}

/// Highlighted primary action button (auto-sized).
pub fn primary_btn(ui: &mut Ui, label: &str) -> Response {
    ui.scope(|ui| {
        let w = &mut ui.style_mut().visuals.widgets;
        w.inactive.weak_bg_fill = Palette::ACCENT_PRIMARY;
        w.inactive.bg_fill = Palette::ACCENT_PRIMARY;
        w.inactive.bg_stroke = Stroke::new(1.0, Palette::BORDER_FOCUS);
        w.hovered.weak_bg_fill = Palette::ACCENT_PRIMARY_HOVER;
        w.hovered.bg_fill = Palette::ACCENT_PRIMARY_HOVER;
        w.hovered.bg_stroke = Stroke::new(1.0, Palette::BORDER_FOCUS);

        ui.button(RichText::new(label).strong().color(Color32::WHITE))
    })
    .inner
}

/// Subtle crimson-tinted destructive button (e.g., `Remove`, `Stop Account`).
pub fn danger_button(ui: &mut Ui, label: &str, enabled: bool, size: [f32; 2]) -> Response {
    ui.scope(|ui| {
        if enabled {
            let w = &mut ui.style_mut().visuals.widgets;
            w.inactive.weak_bg_fill = Palette::DANGER_BG;
            w.inactive.bg_fill = Palette::DANGER_BG;
            w.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(130, 52, 62));
            w.hovered.weak_bg_fill = Color32::from_rgb(94, 34, 42);
            w.hovered.bg_stroke = Stroke::new(1.0, Palette::DANGER);
        }
        let text = if enabled {
            RichText::new(label).color(Palette::DANGER)
        } else {
            RichText::new(label)
        };
        ui.add_enabled(enabled, egui::Button::new(text).min_size(size.into()))
    })
    .inner
}

/// Reusable 2-option segmented pill switch (e.g., `[ HP | MP ]`, `[ <= Lower | >= Greater ]`).
/// `value == false` selects the left segment; `value == true` selects the right segment.
pub fn segmented_switch(
    ui: &mut Ui,
    value: &mut bool,
    left_label: &str,
    right_label: &str,
    total_width: f32,
) -> Response {
    let height = 21.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(total_width, height), egui::Sense::click());

    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        *value = pos.x >= rect.center().x;
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let radius = CornerRadius::same(5);

        // Outer pill track
        painter.rect(
            rect,
            radius,
            Palette::BG_INPUT,
            Stroke::new(1.0, Palette::BORDER_CONTROL),
            egui::StrokeKind::Inside,
        );

        let mid_x = rect.center().x;
        let left_rect = egui::Rect::from_min_max(rect.min, egui::pos2(mid_x, rect.max.y));
        let right_rect = egui::Rect::from_min_max(egui::pos2(mid_x, rect.min.y), rect.max);
        let active_rect = if *value { right_rect } else { left_rect }.shrink(1.0);

        // Active glowing pill
        painter.rect(
            active_rect,
            CornerRadius::same(4),
            Palette::ACCENT_PRIMARY,
            Stroke::new(1.0, Palette::BORDER_FOCUS),
            egui::StrokeKind::Inside,
        );

        let font = egui::FontId::proportional(11.5);
        let left_color = if !*value {
            Color32::WHITE
        } else {
            Palette::TEXT_MUTED
        };
        let right_color = if *value {
            Color32::WHITE
        } else {
            Palette::TEXT_MUTED
        };

        painter.text(
            left_rect.center(),
            egui::Align2::CENTER_CENTER,
            left_label,
            font.clone(),
            left_color,
        );
        painter.text(
            right_rect.center(),
            egui::Align2::CENTER_CENTER,
            right_label,
            font,
            right_color,
        );
    }

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Standard font size (in logical points) for all reusable list item rows.
pub const LIST_ITEM_FONT_SIZE: f32 = 11.5;

/// Framed dark list container (`BG_SURFACE` + `BORDER_SUBTLE`) with optional fixed height.
pub fn list_box(ui: &mut Ui, height: Option<f32>, add_contents: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(Palette::BG_SURFACE)
        .stroke(Stroke::new(1.0, Palette::BORDER_SUBTLE))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            if let Some(h) = height {
                ui.set_min_height(h);
                ui.set_max_height(h);
            }
            add_contents(ui);
        });
}

/// Full-width list row with custom background fill and optional left indent.
/// Does NOT overlay a click layer on top of child widgets so inner buttons/checkboxes work cleanly.
pub fn list_row_with_bg(
    ui: &mut Ui,
    bg: Color32,
    indent_px: i8,
    add_contents: impl FnOnce(&mut Ui),
) -> Response {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin {
            left: 6_i8.saturating_add(indent_px),
            right: 6,
            top: 3,
            bottom: 3,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add_contents(ui);
        })
        .response
}

/// Reusable full-width list row with odd-even zebra striping and selected highlight.
pub fn list_row(
    ui: &mut Ui,
    row_idx: usize,
    selected: bool,
    indent_px: i8,
    add_contents: impl FnOnce(&mut Ui),
) -> Response {
    let bg = if selected {
        Color32::from_rgb(16, 82, 118)
    } else if row_idx % 2 == 1 {
        Color32::from_rgba_unmultiplied(255, 255, 255, 6)
    } else {
        Color32::TRANSPARENT
    };
    list_row_with_bg(ui, bg, indent_px, add_contents)
}

/// Reusable full-width selectable text row (`LIST_ITEM_FONT_SIZE` = 11.5pt, odd-even zebra).
pub fn list_text_row(
    ui: &mut Ui,
    row_idx: usize,
    selected: bool,
    indent_px: i8,
    text: impl Into<String>,
    color: Option<Color32>,
) -> Response {
    let mut rt = RichText::new(text.into()).size(LIST_ITEM_FONT_SIZE);
    if selected {
        rt = rt.color(Color32::WHITE);
    } else if let Some(c) = color {
        rt = rt.color(c);
    }
    list_row(ui, row_idx, selected, indent_px, |ui| {
        ui.add(egui::Label::new(rt).selectable(false));
    })
    .interact(egui::Sense::click())
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Reusable ON/OFF toggle button with crisp green active state.
pub fn toggle_button(
    ui: &mut Ui,
    active: bool,
    enabled: bool,
    label_off: &str,
    label_on: &str,
    size: Option<[f32; 2]>,
) -> Response {
    ui.scope(|ui| {
        if active {
            let w = &mut ui.style_mut().visuals.widgets;
            w.inactive.weak_bg_fill = Palette::SUCCESS_BG;
            w.inactive.bg_fill = Palette::SUCCESS_BG;
            w.inactive.bg_stroke = Stroke::new(1.0, Palette::SUCCESS);
            w.hovered.weak_bg_fill = Color32::from_rgb(26, 82, 54);
            w.hovered.bg_stroke = Stroke::new(1.0, Palette::SUCCESS);
        }
        let text = if active {
            RichText::new(label_on).strong().color(Palette::SUCCESS)
        } else {
            RichText::new(label_off)
        };
        let mut btn = egui::Button::new(text);
        if let Some(s) = size {
            btn = btn.min_size(s.into());
        }
        ui.add_enabled(enabled, btn)
    })
    .inner
}
