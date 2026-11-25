//! The map editor: toolbar, nation legend and calibration panel, and the canvas.

use std::collections::BTreeMap;

use egui::{Align2, Color32, FontId, Key, Modifiers, PointerButton, Rect, Sense, Stroke, StrokeKind};
use uuid::Uuid;

use crate::backend::{ChunkPos, ChunkRect, NationInfo, Project, CHUNK_BLOCKS};
use crate::frontend::map_overlay::ORPHAN_COLOR;
use crate::frontend::map_state::{Gesture, MapSession, MapTool, Projection, View, MAX_BRUSH};
use crate::frontend::session::Session;
use crate::frontend::UiAction;

/// Chunks smaller than this on screen get no grid.
const MIN_GRID_CHUNK_PX: f64 = 8.0;
/// Region (512 block) lines appear once a chunk is at least this large.
const MIN_REGION_CHUNK_PX: f64 = 1.5;
const CHUNKS_PER_REGION: i32 = 32;

pub(crate) fn show(ui: &mut egui::Ui, project: &Project, session: &mut Session) -> UiAction {
    let Some(map) = session.map.as_mut() else {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("Select a map from the tree").weak());
        });
        return UiAction::None;
    };
    let nations = project.nations();
    let colors: BTreeMap<Uuid, [u8; 3]> = nations.iter().map(|n| (n.id, n.color)).collect();
    map.ensure_textures(ui.ctx());
    map.overlay.set_colors(&colors);
    map.overlay.refresh(ui.ctx(), map.editor.layer());

    let mut action = UiAction::None;
    egui::Panel::top("map_toolbar").show_inside(ui, |ui| {
        action = toolbar(ui, map);
    });
    egui::Panel::right("map_side")
        .resizable(true)
        .default_size(280.0)
        .show_inside(ui, |ui| {
            let next = side_panel(ui, map, &nations);
            action = merge(action.clone(), next);
        });
    egui::CentralPanel::default().show_inside(ui, |ui| canvas(ui, map, &nations));
    shortcuts(ui, map);
    action
}

fn merge(current: UiAction, next: UiAction) -> UiAction {
    if matches!(current, UiAction::None) {
        next
    } else {
        current
    }
}

fn toolbar(ui: &mut egui::Ui, map: &mut MapSession) -> UiAction {
    let mut action = UiAction::None;
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        for tool in MapTool::ALL {
            if ui
                .selectable_label(map.tool == tool, tool.label())
                .on_hover_text(tool.hint())
                .clicked()
            {
                map.tool = tool;
            }
        }
        ui.separator();
        let sized = matches!(map.tool, MapTool::Brush | MapTool::Eraser);
        ui.add_enabled_ui(sized, |ui| {
            ui.label("Brush");
            ui.add(egui::Slider::new(&mut map.brush_size, 1..=MAX_BRUSH).suffix(" chunks"));
        });
        ui.separator();
        ui.checkbox(&mut map.show_grid, "Grid");
        if ui.button("Fit").on_hover_text("Show the whole map").clicked() {
            map.fit_pending = true;
        }
        ui.separator();
        if ui
            .add_enabled(map.editor.can_undo(), egui::Button::new("Undo"))
            .on_hover_text("Ctrl+Z")
            .clicked()
        {
            map.undo();
        }
        if ui
            .add_enabled(map.editor.can_redo(), egui::Button::new("Redo"))
            .on_hover_text("Ctrl+Shift+Z")
            .clicked()
        {
            map.redo();
        }
        if ui
            .add_enabled(map.dirty, egui::Button::new("Save map"))
            .on_hover_text("Ctrl+S")
            .clicked()
        {
            action = UiAction::Save;
        }
        if map.dirty {
            ui.label(egui::RichText::new("unsaved changes").small().weak());
        }
    });
    if map.message.is_empty() {
        ui.label(egui::RichText::new(map.tool.hint()).small().weak());
    } else {
        ui.colored_label(ui.visuals().warn_fg_color, &map.message);
    }
    ui.add_space(2.0);
    action
}

fn side_panel(ui: &mut egui::Ui, map: &mut MapSession, nations: &[NationInfo]) -> UiAction {
    let mut action = UiAction::None;
    egui::ScrollArea::vertical()
        .id_salt("map_side_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label(egui::RichText::new(&map.meta.name).size(20.0).strong());
            if !map.meta.dimension.is_empty() {
                ui.label(egui::RichText::new(&map.meta.dimension).weak());
            }
            ui.separator();
            action = legend(ui, map, nations);
            ui.separator();
            calibration(ui, map);
        });
    action
}

fn legend(ui: &mut egui::Ui, map: &mut MapSession, nations: &[NationInfo]) -> UiAction {
    let mut action = UiAction::None;
    ui.label(egui::RichText::new("Nations").strong());
    if nations.is_empty() {
        ui.label(
            egui::RichText::new("Create a Nation page to start claiming chunks.").weak(),
        );
    }
    let counts = map.counts().clone();
    for nation in nations {
        ui.horizontal(|ui| {
            swatch(ui, nation.color);
            let selected = map.active_nation == Some(nation.id);
            if ui.selectable_label(selected, &nation.title).clicked() {
                map.active_nation = Some(nation.id);
                map.message.clear();
            }
            if let Some(count) = counts.get(&nation.id) {
                ui.label(egui::RichText::new(area_text(*count)).small().weak());
            }
            if ui.small_button("Open").on_hover_text("Open the nation page").clicked() {
                action = UiAction::OpenFloating(nation.id);
            }
        });
    }
    let orphans: Vec<(Uuid, usize)> = counts
        .iter()
        .filter(|(id, _)| !nations.iter().any(|nation| nation.id == **id))
        .map(|(id, count)| (*id, *count))
        .collect();
    for (id, count) in orphans {
        ui.horizontal(|ui| {
            swatch(ui, ORPHAN_COLOR);
            let short: String = id.to_string().chars().take(8).collect();
            ui.label(format!("Unknown nation {short}"));
            ui.label(egui::RichText::new(area_text(count)).small().weak());
            if ui
                .small_button("Clear")
                .on_hover_text("Remove these chunks. The nation page no longer exists.")
                .clicked()
            {
                map.clear_nation(id);
            }
        });
    }
    action
}

fn area_text(chunks: usize) -> String {
    let side = u64::try_from(CHUNK_BLOCKS).unwrap_or(16);
    let blocks = u64::try_from(chunks).unwrap_or(0) * side * side;
    format!("{chunks} chunks, {blocks} blocks²")
}

fn swatch(ui: &mut egui::Ui, [red, green, blue]: [u8; 3]) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 3.0, Color32::from_rgb(red, green, blue));
}

fn calibration(ui: &mut egui::Ui, map: &mut MapSession) {
    egui::CollapsingHeader::new("Calibration")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "Tie the image to Minecraft coordinates. Xaero's map images carry none.",
                )
                .small()
                .weak(),
            );
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Pixels per block");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut map.meta.px_per_block)
                            .speed(0.01)
                            .range(0.001..=64.0),
                    )
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Top-left block X");
                changed |= ui
                    .add(egui::DragValue::new(&mut map.meta.origin_block[0]).speed(1.0))
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Top-left block Z");
                changed |= ui
                    .add(egui::DragValue::new(&mut map.meta.origin_block[1]).speed(1.0))
                    .changed();
            });
            if changed {
                map.mark_meta_changed();
            }
            ui.separator();
            ui.label("Reference points");
            let label = if map.picking_reference {
                "Click a pixel on the map..."
            } else {
                "Pick a pixel"
            };
            if ui.selectable_label(map.picking_reference, label).clicked() {
                map.picking_reference = !map.picking_reference;
            }
            for (index, point) in map.references.iter_mut().enumerate() {
                ui.label(format!(
                    "{}. pixel {:.0}, {:.0}",
                    index + 1,
                    point.pixel[0],
                    point.pixel[1]
                ));
                ui.horizontal(|ui| {
                    ui.label("is block X");
                    ui.add(egui::DragValue::new(&mut point.block[0]).speed(1.0));
                    ui.label("Z");
                    ui.add(egui::DragValue::new(&mut point.block[1]).speed(1.0));
                });
            }
            if !map.references.is_empty() {
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        match map.apply_references() {
                            Ok(()) => map.message.clear(),
                            Err(err) => map.message = err.to_string(),
                        }
                    }
                    if ui.button("Clear").clicked() {
                        map.references.clear();
                    }
                });
            }
            ui.label(
                egui::RichText::new("One point sets the origin. Two points also set the scale.")
                    .small()
                    .weak(),
            );
        });
}

fn canvas(ui: &mut egui::Ui, map: &mut MapSession, nations: &[NationInfo]) {
    let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let canvas = response.rect;
    if map.fit_pending && canvas.width() > 1.0 && canvas.height() > 1.0 {
        map.view = View::fitted(map.meta.image_size, canvas.size());
        map.fit_pending = false;
    }
    navigate(ui, map, &response);
    let projection = Projection::new(&map.meta, &map.view, canvas.min);

    painter.rect_filled(canvas, 0.0, Color32::from_gray(24));
    paint_image(&painter, map, &projection);
    map.overlay.paint(&painter, &projection, canvas);
    if map.show_grid {
        paint_grid(&painter, map, &projection, canvas);
    }
    handle_pointer(ui, map, &response, &projection);
    let panning = pan_held(ui, map);
    paint_tool_preview(&painter, map, &projection, &response, nations, panning);
    paint_references(&painter, map, &projection);
    paint_status(&painter, map, &projection, &response, nations, canvas);
}

/// Wheel zoom and drag panning.
fn navigate(ui: &egui::Ui, map: &mut MapSession, response: &egui::Response) {
    if response.hovered() {
        let (scroll, pinch) = ui.input(|input| (input.smooth_scroll_delta().y, input.zoom_delta()));
        let factor = f64::from(pinch) * (f64::from(scroll) * 0.0015).exp();
        if let Some(pos) = response.hover_pos().filter(|_| (factor - 1.0).abs() > 1e-6) {
            map.view.zoom_at(factor, pos - response.rect.min);
        }
    }
    let panning = pan_held(ui, map);
    let drag = response.dragged_by(PointerButton::Middle)
        || response.dragged_by(PointerButton::Secondary)
        || (panning && response.dragged_by(PointerButton::Primary));
    if drag {
        let delta = response.drag_delta();
        map.view.origin_px[0] -= f64::from(delta.x) / map.view.zoom;
        map.view.origin_px[1] -= f64::from(delta.y) / map.view.zoom;
    }
    if response.hovered() {
        let icon = if drag {
            egui::CursorIcon::Grabbing
        } else if panning {
            egui::CursorIcon::Grab
        } else {
            egui::CursorIcon::Crosshair
        };
        ui.ctx().set_cursor_icon(icon);
    }
}

fn pan_held(ui: &egui::Ui, map: &MapSession) -> bool {
    map.tool == MapTool::Pan || ui.input(|input| input.key_down(Key::Space))
}

fn paint_image(painter: &egui::Painter, map: &MapSession, projection: &Projection) {
    let uv = Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
    for tile in &map.tiles {
        let Some(texture) = &tile.texture else {
            continue;
        };
        let min = projection.pixel_to_screen(tile.origin_px[0] as f64, tile.origin_px[1] as f64);
        let max = projection.pixel_to_screen(
            (tile.origin_px[0] + tile.size_px[0]) as f64,
            (tile.origin_px[1] + tile.size_px[1]) as f64,
        );
        painter.image(texture.id(), Rect::from_min_max(min, max), uv, Color32::WHITE);
    }
}

fn paint_grid(painter: &egui::Painter, map: &MapSession, projection: &Projection, canvas: Rect) {
    let size = projection.chunk_screen_size();
    if size < MIN_REGION_CHUNK_PX {
        return;
    }
    let extent = map.extent();
    let visible = ChunkRect::from_corners(
        projection.screen_to_chunk(canvas.min),
        projection.screen_to_chunk(canvas.max),
    );
    let Some(area) = visible.clipped_to(extent) else {
        return;
    };
    let chunk_alpha: u8 = if size >= 24.0 { 70 } else { 40 };
    let chunk_stroke = Stroke::new(1.0, Color32::from_white_alpha(chunk_alpha));
    let region_stroke = Stroke::new(1.5, Color32::from_white_alpha(110));
    let (left, right) = (area.min.x, area.max.x + 1);
    let (top, bottom) = (area.min.z, area.max.z + 1);
    for x in left..=right {
        let region = x.rem_euclid(CHUNKS_PER_REGION) == 0;
        if region || size >= MIN_GRID_CHUNK_PX {
            let from = projection.chunk_to_screen(f64::from(x), f64::from(top));
            let to = projection.chunk_to_screen(f64::from(x), f64::from(bottom));
            painter.line_segment([from, to], if region { region_stroke } else { chunk_stroke });
        }
    }
    for z in top..=bottom {
        let region = z.rem_euclid(CHUNKS_PER_REGION) == 0;
        if region || size >= MIN_GRID_CHUNK_PX {
            let from = projection.chunk_to_screen(f64::from(left), f64::from(z));
            let to = projection.chunk_to_screen(f64::from(right), f64::from(z));
            painter.line_segment([from, to], if region { region_stroke } else { chunk_stroke });
        }
    }
}

/// Start, continue, and finish tool gestures from pointer input.
fn handle_pointer(
    ui: &egui::Ui,
    map: &mut MapSession,
    response: &egui::Response,
    projection: &Projection,
) {
    let (pressed, down, shift, pointer) = ui.input(|input| {
        (
            input.pointer.button_pressed(PointerButton::Primary),
            input.pointer.primary_down(),
            input.modifiers.shift,
            input.pointer.latest_pos(),
        )
    });
    let panning = pan_held(ui, map);
    if let Some(pos) = response.hover_pos().filter(|_| pressed && !panning) {
        press(map, projection, pos, shift);
    }
    if map.gesture == Gesture::None {
        return;
    }
    if !down {
        map.end_gesture(shift);
        return;
    }
    let Some(pos) = pointer else {
        return;
    };
    let chunk = projection.screen_to_chunk(pos);
    match map.gesture {
        Gesture::Stroke { .. } => map.extend_stroke(chunk),
        Gesture::Rect { anchor, .. } => {
            map.gesture = Gesture::Rect {
                anchor,
                current: chunk,
            };
        }
        Gesture::None => {}
    }
}

fn press(map: &mut MapSession, projection: &Projection, pos: egui::Pos2, shift: bool) {
    if map.picking_reference {
        let (x, y) = projection.screen_to_pixel(pos);
        let size = map.meta.image_size;
        if x >= 0.0 && y >= 0.0 && x < f64::from(size[0]) && y < f64::from(size[1]) {
            map.add_reference([x.floor(), y.floor()]);
            map.picking_reference = false;
        }
        return;
    }
    let chunk = projection.screen_to_chunk(pos);
    match map.tool {
        MapTool::Pan => {}
        MapTool::Brush | MapTool::Eraser => map.begin_stroke(chunk),
        MapTool::Rectangle => {
            map.message.clear();
            map.gesture = Gesture::Rect {
                anchor: chunk,
                current: chunk,
            };
        }
        MapTool::Fill => map.fill(chunk, shift),
        MapTool::Eyedropper => map.pick_nation(chunk),
    }
}

fn nation_color(nations: &[NationInfo], id: Option<Uuid>) -> [u8; 3] {
    id.and_then(|id| nations.iter().find(|nation| nation.id == id))
        .map_or(ORPHAN_COLOR, |nation| nation.color)
}

fn chunk_rect_on_screen(projection: &Projection, rect: ChunkRect) -> Rect {
    Rect::from_min_max(
        projection.chunk_to_screen(f64::from(rect.min.x), f64::from(rect.min.z)),
        projection.chunk_to_screen(f64::from(rect.max.x + 1), f64::from(rect.max.z + 1)),
    )
}

fn paint_tool_preview(
    painter: &egui::Painter,
    map: &MapSession,
    projection: &Projection,
    response: &egui::Response,
    nations: &[NationInfo],
    panning: bool,
) {
    let outline = Stroke::new(1.5, Color32::WHITE);
    if let Gesture::Rect { anchor, current } = map.gesture {
        let rect = chunk_rect_on_screen(projection, ChunkRect::from_corners(anchor, current));
        let [red, green, blue] = nation_color(nations, map.active_nation);
        painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(red, green, blue, 70));
        painter.rect_stroke(rect, 0.0, outline, StrokeKind::Inside);
        return;
    }
    let Some(pos) = response.hover_pos() else {
        return;
    };
    if panning {
        return;
    }
    let chunk = projection.screen_to_chunk(pos);
    let footprint = match map.tool {
        MapTool::Brush | MapTool::Eraser => {
            let size = map.brush_size.clamp(1, MAX_BRUSH);
            let low = (size - 1) / 2;
            let min = ChunkPos::new(chunk.x - low, chunk.z - low);
            ChunkRect::from_corners(min, ChunkPos::new(min.x + size - 1, min.z + size - 1))
        }
        MapTool::Rectangle | MapTool::Fill | MapTool::Eyedropper => {
            ChunkRect::from_corners(chunk, chunk)
        }
        MapTool::Pan => return,
    };
    painter.rect_stroke(
        chunk_rect_on_screen(projection, footprint),
        0.0,
        outline,
        StrokeKind::Outside,
    );
}

fn paint_references(painter: &egui::Painter, map: &MapSession, projection: &Projection) {
    let stroke = Stroke::new(2.0, Color32::YELLOW);
    for (index, point) in map.references.iter().enumerate() {
        let center = projection.pixel_to_screen(point.pixel[0] + 0.5, point.pixel[1] + 0.5);
        painter.circle_stroke(center, 8.0, stroke);
        painter.line_segment([center - egui::vec2(12.0, 0.0), center + egui::vec2(12.0, 0.0)], stroke);
        painter.line_segment([center - egui::vec2(0.0, 12.0), center + egui::vec2(0.0, 12.0)], stroke);
        painter.text(
            center + egui::vec2(10.0, -10.0),
            Align2::LEFT_BOTTOM,
            (index + 1).to_string(),
            FontId::monospace(14.0),
            Color32::YELLOW,
        );
    }
}

fn paint_status(
    painter: &egui::Painter,
    map: &MapSession,
    projection: &Projection,
    response: &egui::Response,
    nations: &[NationInfo],
    canvas: Rect,
) {
    let Some(pos) = response.hover_pos() else {
        return;
    };
    let (x, z) = projection.screen_to_block(pos);
    let chunk = projection.screen_to_chunk(pos);
    let owner = match map.editor.layer().get(chunk) {
        Some(id) => nations
            .iter()
            .find(|nation| nation.id == id)
            .map_or_else(|| "Unknown nation".to_owned(), |nation| nation.title.clone()),
        None => "Unclaimed".to_owned(),
    };
    let text = format!(
        "Block {:.0}, {:.0}  Chunk {}, {}  {owner}",
        x.floor(),
        z.floor(),
        chunk.x,
        chunk.z
    );
    let galley = painter.layout_no_wrap(text, FontId::monospace(14.0), Color32::WHITE);
    let origin = canvas.left_bottom() + egui::vec2(8.0, -8.0 - galley.size().y);
    let backdrop = Rect::from_min_size(origin, galley.size()).expand(4.0);
    painter.rect_filled(backdrop, 3.0, Color32::from_black_alpha(170));
    painter.galley(origin, galley, Color32::WHITE);
}

/// Undo, redo, and tool keys. Ignored while a text field has focus.
fn shortcuts(ui: &egui::Ui, map: &mut MapSession) {
    if ui.ctx().memory(|memory| memory.focused().is_some()) {
        return;
    }
    let (redo, undo, tool) = ui.input_mut(|input| {
        let redo = input.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
            || input.consume_key(Modifiers::COMMAND, Key::Y);
        let undo = input.consume_key(Modifiers::COMMAND, Key::Z);
        let tool = [
            (Key::B, MapTool::Brush),
            (Key::R, MapTool::Rectangle),
            (Key::F, MapTool::Fill),
            (Key::E, MapTool::Eraser),
            (Key::I, MapTool::Eyedropper),
        ]
        .into_iter()
        .find(|(key, _)| input.consume_key(Modifiers::NONE, *key))
        .map(|(_, tool)| tool);
        (redo, undo, tool)
    });
    if redo {
        map.redo();
    } else if undo {
        map.undo();
    }
    if let Some(tool) = tool {
        map.tool = tool;
    }
    if ui.input(|input| input.key_pressed(Key::Escape)) {
        map.picking_reference = false;
    }
}

#[cfg(test)]
mod tests {
    use super::show;
    use crate::backend::{ChunkPos, Frontmatter, Project};
    use crate::frontend::map_state::{MapSession, MapTool};
    use crate::frontend::session::Session;
    use uuid::Uuid;

    fn frame(ctx: &egui::Context, project: &Project, session: &mut Session) {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0)));
        let _ = ctx.run_ui(input, |ui| {
            show(ui, project, session);
        });
    }

    #[test]
    fn map_view_draws_paints_and_undoes() {
        let dir = std::env::temp_dir().join(format!("mot-view-{}", Uuid::new_v4()));
        let mut project = Project::create(&dir, "Campaign", None).expect("project");
        let nation = project
            .create_page(&dir.join("nations"), Frontmatter::nation("Rome".to_owned()), "")
            .expect("nation");
        let source = dir.join("export.png");
        image::RgbaImage::new(512, 512).save(&source).expect("png");
        let slug = project
            .create_map("World", &source, 1.0, [-256.0, -256.0], "Overworld")
            .expect("map");
        let doc = project.load_map(&slug).expect("load");
        let image_path = project.map_dir(&slug).expect("dir").join(&doc.meta.image);

        let mut session = Session::default();
        session.map = Some(MapSession::open(doc, &image_path).expect("open"));
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::empty());
        for _ in 0..3 {
            frame(&ctx, &project, &mut session);
        }

        let map = session.map.as_mut().expect("map");
        assert!(map.tiles.iter().all(|tile| tile.texture.is_some()));
        map.tool = MapTool::Brush;
        map.begin_stroke(ChunkPos::new(0, 0));
        assert!(map.message.contains("Select a nation"), "painting needs a nation");
        map.active_nation = Some(nation);
        map.begin_stroke(ChunkPos::new(0, 0));
        map.extend_stroke(ChunkPos::new(5, 3));
        map.end_gesture(false);
        assert!(map.dirty);
        let painted = map.counts().get(&nation).copied().unwrap_or(0);
        assert!(painted >= 6, "painted {painted}");
        // The extent is chunks -16..=15, so painting outside it is ignored.
        map.begin_stroke(ChunkPos::new(40, 40));
        map.end_gesture(false);
        assert_eq!(map.counts().get(&nation).copied().unwrap_or(0), painted);

        for _ in 0..2 {
            frame(&ctx, &project, &mut session);
        }
        assert_eq!(session.map.as_ref().expect("map").overlay.tile_count(), 1);

        let map = session.map.as_mut().expect("map");
        map.fill(ChunkPos::new(-10, -10), false);
        assert!(map.counts().get(&nation).copied().unwrap_or(0) > painted);
        map.undo();
        assert_eq!(map.counts().get(&nation).copied().unwrap_or(0), painted);
        map.undo();
        assert!(map.counts().is_empty());
        map.redo();
        assert_eq!(map.counts().get(&nation).copied().unwrap_or(0), painted);
        frame(&ctx, &project, &mut session);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
