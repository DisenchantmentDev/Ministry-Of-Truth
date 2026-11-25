//! State and editing logic for the open map. Drawing lives in `map_view`.

use std::collections::BTreeMap;
use std::path::Path;

use egui::{ColorImage, Context, Pos2, TextureHandle, TextureOptions};
use uuid::Uuid;

use crate::backend::{
    BlockPos, BlockRect, ChunkPos, ChunkRect, ClaimEdit, ClaimEditor, MapDoc, MapMeta, MotError,
    Result, CHUNK_BLOCKS,
};
use crate::frontend::map_overlay::OverlayCache;

/// Side of one base-image texture in pixels. Large exports are split so they
/// stay under GPU texture size limits.
const IMAGE_TILE_PX: usize = 2048;
/// Flood fills larger than this are refused instead of freezing the app.
const MAX_FILL_CHUNKS: usize = 250_000;
pub(crate) const MAX_BRUSH: i32 = 32;
pub(crate) const MIN_ZOOM: f64 = 0.02;
pub(crate) const MAX_ZOOM: f64 = 64.0;

/// What the primary mouse button does on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapTool {
    Pan,
    Brush,
    Rectangle,
    Fill,
    Eraser,
    Eyedropper,
}

impl MapTool {
    pub(crate) const ALL: [Self; 6] = [
        Self::Pan,
        Self::Brush,
        Self::Rectangle,
        Self::Fill,
        Self::Eraser,
        Self::Eyedropper,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Pan => "Pan",
            Self::Brush => "Brush",
            Self::Rectangle => "Rectangle",
            Self::Fill => "Fill",
            Self::Eraser => "Eraser",
            Self::Eyedropper => "Pick",
        }
    }

    pub(crate) fn hint(self, blocks: bool) -> &'static str {
        match self {
            Self::Pan => "Drag to move the map. Middle or right drag also pans with any tool.",
            Self::Brush if blocks => "Drag to claim individual blocks. Key: B",
            Self::Brush => "Drag to claim chunks for the selected nation. Key: B",
            Self::Rectangle if blocks => "Drag a box of blocks. Hold Shift to clear instead. Key: R",
            Self::Rectangle => "Drag a box of chunks. Hold Shift to clear instead. Key: R",
            Self::Fill if blocks => {
                "Fill the area inside a border of blocks. Hold Shift to clear instead. Key: F"
            }
            Self::Fill => "Fill a connected area. Hold Shift to clear instead. Key: F",
            Self::Eraser if blocks => "Drag to clear individual blocks. Key: E",
            Self::Eraser => "Drag to clear chunks. Key: E",
            Self::Eyedropper if blocks => "Click a block to select its nation. Key: I",
            Self::Eyedropper => "Click a claimed chunk to select its nation. Key: I",
        }
    }
}

/// What a painting action writes into a chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    Claim(Uuid),
    Clear,
}

impl Paint {
    fn owner(self) -> Option<Uuid> {
        match self {
            Self::Claim(id) => Some(id),
            Self::Clear => None,
        }
    }
}

/// An in-progress canvas drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    None,
    Stroke {
        last: ChunkPos,
        paint: Paint,
    },
    Rect {
        anchor: ChunkPos,
        current: ChunkPos,
    },
    BlockStroke {
        last: BlockPos,
        paint: Paint,
    },
    BlockRect {
        anchor: BlockPos,
        current: BlockPos,
    },
}

/// A pixel the user has matched to a known block position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReferencePoint {
    pub pixel: [f64; 2],
    pub block: [f64; 2],
}

/// Maps between image pixels, blocks, chunks, and screen positions.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Projection {
    origin_block: [f64; 2],
    px_per_block: f64,
    view_origin_px: [f64; 2],
    zoom: f64,
    canvas_min: Pos2,
}

impl Projection {
    pub(crate) fn new(meta: &MapMeta, view: &View, canvas_min: Pos2) -> Self {
        Self {
            origin_block: meta.origin_block,
            px_per_block: meta.px_per_block,
            view_origin_px: view.origin_px,
            zoom: view.zoom,
            canvas_min,
        }
    }

    pub(crate) fn pixel_to_screen(&self, px: f64, py: f64) -> Pos2 {
        Pos2::new(
            self.canvas_min.x + ((px - self.view_origin_px[0]) * self.zoom) as f32,
            self.canvas_min.y + ((py - self.view_origin_px[1]) * self.zoom) as f32,
        )
    }

    pub(crate) fn screen_to_pixel(&self, pos: Pos2) -> (f64, f64) {
        (
            self.view_origin_px[0] + f64::from(pos.x - self.canvas_min.x) / self.zoom,
            self.view_origin_px[1] + f64::from(pos.y - self.canvas_min.y) / self.zoom,
        )
    }

    pub(crate) fn block_to_screen(&self, x: f64, z: f64) -> Pos2 {
        self.pixel_to_screen(
            (x - self.origin_block[0]) * self.px_per_block,
            (z - self.origin_block[1]) * self.px_per_block,
        )
    }

    /// Screen position of a point given in chunk units.
    pub(crate) fn chunk_to_screen(&self, cx: f64, cz: f64) -> Pos2 {
        let size = f64::from(CHUNK_BLOCKS);
        self.block_to_screen(cx * size, cz * size)
    }

    pub(crate) fn screen_to_block(&self, pos: Pos2) -> (f64, f64) {
        let (px, py) = self.screen_to_pixel(pos);
        (
            self.origin_block[0] + px / self.px_per_block,
            self.origin_block[1] + py / self.px_per_block,
        )
    }

    pub(crate) fn screen_to_chunk(&self, pos: Pos2) -> ChunkPos {
        let (x, z) = self.screen_to_block(pos);
        ChunkPos::from_block_f(x, z)
    }

    pub(crate) fn screen_to_block_pos(&self, pos: Pos2) -> BlockPos {
        let (x, z) = self.screen_to_block(pos);
        BlockPos::from_block_f(x, z)
    }

    /// Edge length of one chunk on screen.
    pub(crate) fn chunk_screen_size(&self) -> f64 {
        f64::from(CHUNK_BLOCKS) * self.px_per_block * self.zoom
    }
}

/// Pan and zoom. `zoom` is screen pixels per image pixel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// Image pixel at the top-left of the canvas.
    pub origin_px: [f64; 2],
    pub zoom: f64,
}

impl View {
    /// Center the whole image in a canvas of the given size.
    pub(crate) fn fitted(image_size: [u32; 2], canvas: egui::Vec2) -> Self {
        let (width, height) = (f64::from(image_size[0]), f64::from(image_size[1]));
        let zoom = (f64::from(canvas.x) / width)
            .min(f64::from(canvas.y) / height)
            .clamp(MIN_ZOOM, MAX_ZOOM);
        Self {
            origin_px: [
                (width - f64::from(canvas.x) / zoom) / 2.0,
                (height - f64::from(canvas.y) / zoom) / 2.0,
            ],
            zoom,
        }
    }

    /// Zoom by `factor`, keeping the image pixel under `anchor` (canvas-relative) fixed.
    pub(crate) fn zoom_at(&mut self, factor: f64, anchor: egui::Vec2) {
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let (ax, ay) = (f64::from(anchor.x), f64::from(anchor.y));
        let pixel = [
            self.origin_px[0] + ax / self.zoom,
            self.origin_px[1] + ay / self.zoom,
        ];
        self.origin_px = [pixel[0] - ax / zoom, pixel[1] - ay / zoom];
        self.zoom = zoom;
    }
}

/// One texture of the base image.
pub(crate) struct ImageTile {
    /// Top-left pixel of this tile in the full image.
    pub origin_px: [usize; 2],
    pub size_px: [usize; 2],
    pending: Option<ColorImage>,
    pub texture: Option<TextureHandle>,
}

impl ImageTile {
    fn upload(&mut self, ctx: &Context, name: &str) {
        if let Some(image) = self.pending.take() {
            let options = TextureOptions {
                magnification: egui::TextureFilter::Nearest,
                minification: egui::TextureFilter::Linear,
                ..TextureOptions::LINEAR
            };
            self.texture = Some(ctx.load_texture(name, image, options));
        }
    }
}

/// Everything the map view needs while a map is open.
pub struct MapSession {
    pub slug: String,
    pub meta: MapMeta,
    pub editor: ClaimEditor,
    pub dirty: bool,
    pub tool: MapTool,
    pub brush_size: i32,
    /// When set, brush, rectangle, fill, and eraser work on single blocks.
    pub block_claims: bool,
    pub active_nation: Option<Uuid>,
    pub show_grid: bool,
    pub view: View,
    pub(crate) fit_pending: bool,
    pub(crate) tiles: Vec<ImageTile>,
    pub(crate) overlay: OverlayCache,
    pub gesture: Gesture,
    pub picking_reference: bool,
    pub references: Vec<ReferencePoint>,
    pub message: String,
    counts: BTreeMap<Uuid, u64>,
    counts_dirty: bool,
}

impl MapSession {
    /// Decode the base image and prepare the session.
    pub(crate) fn open(doc: MapDoc, image_path: &Path) -> Result<Self> {
        let image = image::open(image_path)
            .map_err(|err| MotError::new(format!("cannot read map image: {err}")))?
            .to_rgba8();
        let width = usize::try_from(image.width()).unwrap_or(0);
        let height = usize::try_from(image.height()).unwrap_or(0);
        let mut meta = doc.meta;
        let actual = [image.width(), image.height()];
        if meta.image_size != actual {
            meta.image_size = actual;
        }
        let mut tiles = Vec::new();
        for y in (0..height).step_by(IMAGE_TILE_PX) {
            for x in (0..width).step_by(IMAGE_TILE_PX) {
                tiles.push(cut_tile(&image, width, [x, y], [width, height]));
            }
        }
        let mut overlay = OverlayCache::default();
        overlay.mark_layer(&doc.claims);
        Ok(Self {
            slug: doc.slug,
            meta,
            editor: ClaimEditor::new(doc.claims),
            dirty: false,
            tool: MapTool::Brush,
            brush_size: 1,
            block_claims: false,
            active_nation: None,
            show_grid: true,
            view: View {
                origin_px: [0.0, 0.0],
                zoom: 1.0,
            },
            fit_pending: true,
            tiles,
            overlay,
            gesture: Gesture::None,
            picking_reference: false,
            references: Vec::new(),
            message: String::new(),
            counts: BTreeMap::new(),
            counts_dirty: true,
        })
    }

    /// Upload any base-image tiles that are still waiting for a texture.
    pub(crate) fn ensure_textures(&mut self, ctx: &Context) {
        for (index, tile) in self.tiles.iter_mut().enumerate() {
            tile.upload(ctx, &format!("map_{}_{index}", self.slug));
        }
    }

    pub(crate) fn extent(&self) -> ChunkRect {
        self.meta.chunk_extent()
    }

    pub(crate) fn block_extent(&self) -> BlockRect {
        self.meta.block_extent()
    }

    /// Claimed blocks per nation, recomputed only after edits.
    pub(crate) fn counts(&mut self) -> &BTreeMap<Uuid, u64> {
        if self.counts_dirty {
            self.counts = self.editor.layer().block_area();
            self.counts_dirty = false;
        }
        &self.counts
    }

    pub(crate) fn mark_meta_changed(&mut self) {
        self.dirty = true;
    }

    // ---- Painting -------------------------------------------------------

    /// The selected nation, or `None` (with a hint) when none is selected.
    fn nation_paint(&mut self) -> Option<Paint> {
        if let Some(id) = self.active_nation {
            return Some(Paint::Claim(id));
        }
        self.message = "Select a nation in the legend first.".to_owned();
        None
    }

    /// Owner written by the brush, or `None` when it cannot paint yet.
    fn brush_paint(&mut self) -> Option<Paint> {
        if self.tool == MapTool::Eraser {
            Some(Paint::Clear)
        } else {
            self.nation_paint()
        }
    }

    pub(crate) fn begin_stroke(&mut self, at: ChunkPos) {
        let Some(paint) = self.brush_paint() else {
            return;
        };
        self.message.clear();
        self.stamp(at, paint.owner());
        self.gesture = Gesture::Stroke { last: at, paint };
    }

    /// Continue a brush stroke, filling the gap since the last position.
    pub(crate) fn extend_stroke(&mut self, to: ChunkPos) {
        let Gesture::Stroke { last, paint } = self.gesture else {
            return;
        };
        if last == to {
            return;
        }
        for pos in line_between(last, to).into_iter().skip(1) {
            self.stamp(pos, paint.owner());
        }
        self.gesture = Gesture::Stroke { last: to, paint };
    }

    pub(crate) fn end_gesture(&mut self, clear: bool) {
        match self.gesture {
            Gesture::Rect { anchor, current } => {
                let paint = if clear { Some(Paint::Clear) } else { self.nation_paint() };
                if let Some(paint) = paint {
                    let rect = ChunkRect::from_corners(anchor, current);
                    if let Some(rect) = rect.clipped_to(self.extent()) {
                        for pos in rect.positions() {
                            self.paint_chunk(pos, paint.owner());
                        }
                    }
                }
            }
            Gesture::BlockRect { anchor, current } => {
                let paint = if clear { Some(Paint::Clear) } else { self.nation_paint() };
                if let Some(paint) = paint {
                    self.paint_block_rect(BlockRect::from_corners(anchor, current), paint.owner());
                }
            }
            Gesture::Stroke { .. } | Gesture::BlockStroke { .. } | Gesture::None => {}
        }
        self.editor.commit();
        self.gesture = Gesture::None;
    }

    /// Drop an in-progress rectangle without painting it. A stroke is kept.
    pub(crate) fn cancel_gesture(&mut self) {
        if matches!(
            self.gesture,
            Gesture::Stroke { .. } | Gesture::BlockStroke { .. }
        ) {
            self.editor.commit();
        }
        self.gesture = Gesture::None;
    }

    pub(crate) fn begin_block_stroke(&mut self, at: BlockPos) {
        let Some(paint) = self.brush_paint() else {
            return;
        };
        self.message.clear();
        self.stamp_blocks(at, paint.owner());
        self.gesture = Gesture::BlockStroke { last: at, paint };
    }

    pub(crate) fn extend_block_stroke(&mut self, to: BlockPos) {
        let Gesture::BlockStroke { last, paint } = self.gesture else {
            return;
        };
        if last == to {
            return;
        }
        for (x, z) in line_between_xy((last.x, last.z), (to.x, to.z))
            .into_iter()
            .skip(1)
        {
            self.stamp_blocks(BlockPos::new(x, z), paint.owner());
        }
        self.gesture = Gesture::BlockStroke { last: to, paint };
    }

    pub(crate) fn begin_block_rect(&mut self, at: BlockPos) {
        self.message.clear();
        self.gesture = Gesture::BlockRect {
            anchor: at,
            current: at,
        };
    }

    /// Flood fill the connected area under `at`. `clear` empties it instead.
    pub(crate) fn fill(&mut self, at: ChunkPos, clear: bool) {
        let paint = if clear { Some(Paint::Clear) } else { self.nation_paint() };
        let Some(paint) = paint else {
            return;
        };
        let region = self.editor.layer().connected_region(at, self.extent());
        if region.len() > MAX_FILL_CHUNKS {
            self.message = "That area is too large to fill. Outline it first.".to_owned();
            return;
        }
        self.message.clear();
        for pos in region {
            self.paint_chunk(pos, paint.owner());
        }
        self.editor.commit();
    }

    /// Flood fill connected blocks of the same owner. A border of claimed blocks stops it.
    pub(crate) fn fill_block(&mut self, at: BlockPos, clear: bool) {
        let paint = if clear { Some(Paint::Clear) } else { self.nation_paint() };
        let Some(paint) = paint else {
            return;
        };
        let Some(region) = self
            .editor
            .layer()
            .block_region(at, self.block_extent(), MAX_FILL_CHUNKS)
        else {
            self.message = "That area is too large to fill. Outline it first.".to_owned();
            return;
        };
        self.message.clear();
        for pos in region.chunks {
            self.paint_chunk(pos, paint.owner());
        }
        for pos in region.blocks {
            self.paint_block(pos, paint.owner());
        }
        self.editor.commit();
    }

    /// Select the nation that owns the block under the cursor.
    pub(crate) fn pick_block(&mut self, at: BlockPos) {
        if let Some(owner) = self.editor.layer().block_owner(at) {
            self.active_nation = Some(owner);
            self.message.clear();
        }
    }

    /// Select the nation that owns the chunk under the cursor.
    pub(crate) fn pick_nation(&mut self, at: ChunkPos) {
        if let Some(owner) = self.editor.layer().get(at) {
            self.active_nation = Some(owner);
            self.message.clear();
        }
    }

    pub(crate) fn undo(&mut self) {
        let changes = self.editor.undo();
        self.after_history(&changes);
    }

    pub(crate) fn redo(&mut self) {
        let changes = self.editor.redo();
        self.after_history(&changes);
    }

    /// Remove every chunk of a nation that no longer has a page.
    pub(crate) fn clear_nation(&mut self, owner: Uuid) {
        let changes = self.editor.clear_owner(owner);
        self.after_history(&changes);
        if self.active_nation == Some(owner) {
            self.active_nation = None;
        }
    }

    fn after_history(&mut self, changes: &[ClaimEdit]) {
        if changes.is_empty() {
            return;
        }
        for change in changes {
            let chunk = match *change {
                ClaimEdit::Chunk(change) => change.pos,
                ClaimEdit::Block(change) => change.pos.chunk(),
            };
            self.overlay.mark_detail(chunk);
        }
        self.dirty = true;
        self.counts_dirty = true;
    }

    /// Paint a square brush footprint centered on `center`.
    fn stamp(&mut self, center: ChunkPos, owner: Option<Uuid>) {
        let size = self.brush_size.clamp(1, MAX_BRUSH);
        let low = (size - 1) / 2;
        let min = ChunkPos::new(center.x - low, center.z - low);
        let footprint = ChunkRect::from_corners(min, ChunkPos::new(min.x + size - 1, min.z + size - 1));
        if let Some(rect) = footprint.clipped_to(self.extent()) {
            for pos in rect.positions() {
                self.paint_chunk(pos, owner);
            }
        }
    }

    fn paint_block_rect(&mut self, rect: BlockRect, owner: Option<Uuid>) {
        let Some(rect) = rect.clipped_to(self.block_extent()) else {
            return;
        };
        let chunk_min = rect.min.chunk();
        let chunk_max = rect.max.chunk();
        let span = i64::from(chunk_max.x - chunk_min.x + 1) * i64::from(chunk_max.z - chunk_min.z + 1);
        if span > i64::try_from(MAX_FILL_CHUNKS).unwrap_or(i64::MAX) {
            self.message = "That area is too large to fill. Outline it first.".to_owned();
            return;
        }
        self.message.clear();
        let bounds = self.block_extent();
        for z in chunk_min.z..=chunk_max.z {
            for x in chunk_min.x..=chunk_max.x {
                let chunk = ChunkPos::new(x, z);
                let Some(chunk_blocks) = BlockRect::of_chunk(chunk) else {
                    continue;
                };
                if rect.contains_rect(chunk_blocks) && bounds.contains_rect(chunk_blocks) {
                    self.paint_chunk(chunk, owner);
                } else {
                    for pos in chunk_blocks.positions() {
                        if rect.contains(pos) && bounds.contains(pos) {
                            self.paint_block(pos, owner);
                        }
                    }
                }
            }
        }
    }

    fn stamp_blocks(&mut self, center: BlockPos, owner: Option<Uuid>) {
        let size = self.brush_size.clamp(1, MAX_BRUSH);
        let low = (size - 1) / 2;
        let min = BlockPos::new(center.x - low, center.z - low);
        let footprint =
            BlockRect::from_corners(min, BlockPos::new(min.x + size - 1, min.z + size - 1));
        if let Some(rect) = footprint.clipped_to(self.block_extent()) {
            for pos in rect.positions() {
                self.paint_block(pos, owner);
            }
        }
    }

    fn paint_block(&mut self, pos: BlockPos, owner: Option<Uuid>) {
        if self.editor.paint_block(pos, owner) {
            self.overlay.mark_detail(pos.chunk());
            self.dirty = true;
            self.counts_dirty = true;
        }
    }

    fn paint_chunk(&mut self, pos: ChunkPos, owner: Option<Uuid>) {
        if self.editor.paint(pos, owner) {
            self.overlay.mark_detail(pos);
            self.dirty = true;
            self.counts_dirty = true;
        }
    }

    // ---- Calibration ----------------------------------------------------

    /// Record a reference pixel. The block position starts at the current guess.
    pub(crate) fn add_reference(&mut self, pixel: [f64; 2]) {
        let (x, z) = self.meta.pixel_to_block(pixel[0], pixel[1]);
        if self.references.len() >= 2 {
            self.references.remove(0);
        }
        self.references.push(ReferencePoint {
            pixel,
            block: [x.round(), z.round()],
        });
    }

    /// Recompute scale (with two points) and origin from the reference points.
    pub(crate) fn apply_references(&mut self) -> Result<()> {
        let Some(first) = self.references.first().copied() else {
            return Err(MotError::new("pick at least one reference pixel"));
        };
        let scale = match self.references.get(1).copied() {
            Some(second) => scale_between(first, second)
                .ok_or_else(|| MotError::new("the two points need different block positions"))?,
            None => self.meta.px_per_block,
        };
        let mut meta = self.meta.clone();
        meta.px_per_block = scale;
        meta.origin_block = [
            first.block[0] - first.pixel[0] / scale,
            first.block[1] - first.pixel[1] / scale,
        ];
        meta.validate()?;
        self.meta = meta;
        self.references.clear();
        self.dirty = true;
        Ok(())
    }
}

/// Pixels per block implied by two reference points.
fn scale_between(first: ReferencePoint, second: ReferencePoint) -> Option<f64> {
    (0..2).find_map(|axis| {
        let pixels = (second.pixel[axis] - first.pixel[axis]).abs();
        let blocks = (second.block[axis] - first.block[axis]).abs();
        (blocks >= 1.0 && pixels > 0.0).then(|| pixels / blocks)
    })
}

fn cut_tile(
    image: &image::RgbaImage,
    width: usize,
    origin: [usize; 2],
    full: [usize; 2],
) -> ImageTile {
    let tile_w = IMAGE_TILE_PX.min(full[0] - origin[0]);
    let tile_h = IMAGE_TILE_PX.min(full[1] - origin[1]);
    let raw = image.as_raw();
    let mut rgba = Vec::with_capacity(tile_w * tile_h * 4);
    for row in origin[1]..origin[1] + tile_h {
        let start = (row * width + origin[0]) * 4;
        rgba.extend_from_slice(&raw[start..start + tile_w * 4]);
    }
    ImageTile {
        origin_px: origin,
        size_px: [tile_w, tile_h],
        pending: Some(ColorImage::from_rgba_unmultiplied([tile_w, tile_h], &rgba)),
        texture: None,
    }
}

/// Chunks on the straight line from `from` to `to`, both included.
fn line_between(from: ChunkPos, to: ChunkPos) -> Vec<ChunkPos> {
    line_between_xy((from.x, from.z), (to.x, to.z))
        .into_iter()
        .map(|(x, z)| ChunkPos::new(x, z))
        .collect()
}

fn line_between_xy(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    let (dx, dz) = ((to.0 - from.0).abs(), -(to.1 - from.1).abs());
    let step_x = if from.0 < to.0 { 1 } else { -1 };
    let step_z = if from.1 < to.1 { 1 } else { -1 };
    let mut error = dx + dz;
    let (mut x, mut z) = from;
    let mut points = vec![from];
    while (x, z) != to {
        let doubled = 2 * error;
        if doubled >= dz {
            error += dz;
            x += step_x;
        }
        if doubled <= dx {
            error += dx;
            z += step_z;
        }
        points.push((x, z));
    }
    points
}

#[cfg(test)]
mod tests {
    use super::{line_between, scale_between, ReferencePoint, View};
    use crate::backend::ChunkPos;

    #[test]
    fn line_has_no_gaps() {
        let points = line_between(ChunkPos::new(0, 0), ChunkPos::new(5, 2));
        assert_eq!(points.first(), Some(&ChunkPos::new(0, 0)));
        assert_eq!(points.last(), Some(&ChunkPos::new(5, 2)));
        for pair in points.windows(2) {
            assert!((pair[1].x - pair[0].x).abs() <= 1 && (pair[1].z - pair[0].z).abs() <= 1);
        }
        assert_eq!(line_between(ChunkPos::new(3, 3), ChunkPos::new(3, 3)).len(), 1);
        let back = line_between(ChunkPos::new(-2, 4), ChunkPos::new(-5, 1));
        assert_eq!(back.len(), 4);
    }

    #[test]
    fn two_references_give_the_scale() {
        let a = ReferencePoint {
            pixel: [100.0, 50.0],
            block: [0.0, 0.0],
        };
        let b = ReferencePoint {
            pixel: [300.0, 50.0],
            block: [400.0, 0.0],
        };
        assert_eq!(scale_between(a, b), Some(0.5));
        let same = ReferencePoint {
            pixel: [300.0, 90.0],
            block: [0.0, 0.0],
        };
        assert_eq!(scale_between(a, same), None);
        let vertical = ReferencePoint {
            pixel: [100.0, 250.0],
            block: [0.0, 100.0],
        };
        assert_eq!(scale_between(a, vertical), Some(2.0), "falls back to the z axis");
    }

    #[test]
    fn zoom_keeps_the_anchor_pixel_fixed() {
        let mut view = View {
            origin_px: [10.0, 20.0],
            zoom: 2.0,
        };
        let anchor = egui::vec2(100.0, 60.0);
        let before = (
            view.origin_px[0] + f64::from(anchor.x) / view.zoom,
            view.origin_px[1] + f64::from(anchor.y) / view.zoom,
        );
        view.zoom_at(1.5, anchor);
        let after = (
            view.origin_px[0] + f64::from(anchor.x) / view.zoom,
            view.origin_px[1] + f64::from(anchor.y) / view.zoom,
        );
        assert!((before.0 - after.0).abs() < 1e-9 && (before.1 - after.1).abs() < 1e-9);
        assert!((view.zoom - 3.0).abs() < 1e-9);
    }
}
