//! Small layout helpers shared by the workspace views.

/// Widest readable content column for pages and the timeline.
pub(crate) const CONTENT_WIDTH: f32 = 860.0;

/// Lay out `add` in a column no wider than `max_width`, centered in `ui`.
///
/// When the pane is narrower than `max_width` the content simply fills it.
pub(crate) fn centered_column<R>(
    ui: &mut egui::Ui,
    max_width: f32,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let available = ui.available_width();
    if available <= max_width {
        return add(ui);
    }
    let spacing = ui.spacing().item_spacing.x;
    let side = ((available - max_width) / 2.0 - spacing).max(0.0);
    ui.horizontal_top(|ui| {
        ui.add_space(side);
        ui.vertical(|ui| {
            ui.set_width(max_width);
            add(ui)
        })
        .inner
    })
    .inner
}
