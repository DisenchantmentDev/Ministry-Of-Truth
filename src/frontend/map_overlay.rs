//! Tinted territory overlay for the map view.
//!
//! Claims are rasterized at one pixel per chunk into square tiles of
//! [`TILE_CHUNKS`] chunks. Each tile is one texture drawn with nearest-neighbor
//! filtering, so the cost of drawing does not grow with the size of a territory.
//! Only tiles touched by an edit are rebuilt. Outlines are kept as merged line
//! runs and drawn on top of the tint.

use std::collections::{BTreeMap, BTreeSet};

use egui::{Color32, ColorImage, Context, Painter, Pos2, Rect, Stroke, TextureHandle, TextureOptions};
use uuid::Uuid;

use crate::backend::{ChunkPos, ClaimLayer};
use crate::frontend::map_state::Projection;

/// Chunks along one side of an overlay tile.
pub(crate) const TILE_CHUNKS: i32 = 256;
/// Opacity of the tint (about 35%).
const FILL_ALPHA: u8 = 90;
/// Width of the outline in screen pixels.
const OUTLINE_WIDTH: f32 = 2.0;
/// Outlines are skipped when a chunk is smaller than this many screen pixels.
const MIN_OUTLINE_CHUNK_PX: f64 = 2.0;
/// Tint for chunks whose nation page no longer exists.
pub(crate) const ORPHAN_COLOR: [u8; 3] = [128, 128, 128];

type TileKey = (i32, i32);

/// Which side of a chunk an outline segment runs along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EdgeSide {
    North,
    South,
    West,
    East,
}

impl EdgeSide {
    const ALL: [Self; 4] = [Self::North, Self::South, Self::West, Self::East];

    fn neighbor(self, pos: ChunkPos) -> ChunkPos {
        match self {
            Self::North => ChunkPos::new(pos.x, pos.z - 1),
            Self::South => ChunkPos::new(pos.x, pos.z + 1),
            Self::West => ChunkPos::new(pos.x - 1, pos.z),
            Self::East => ChunkPos::new(pos.x + 1, pos.z),
        }
    }

    fn horizontal(self) -> bool {
        matches!(self, Self::North | Self::South)
    }
}

/// A straight run of chunk edges with the same owner and side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Edge {
    /// First chunk of the run.
    pub pos: ChunkPos,
    pub side: EdgeSide,
    /// Number of chunks along the run.
    pub len: i32,
    pub owner: Uuid,
}

struct OverlayTile {
    texture: TextureHandle,
    edges: Vec<Edge>,
}

/// Cached textures and outlines for one open map.
#[derive(Default)]
pub(crate) struct OverlayCache {
    tiles: BTreeMap<TileKey, OverlayTile>,
    dirty: BTreeSet<TileKey>,
    colors: BTreeMap<Uuid, [u8; 3]>,
}

impl OverlayCache {
    /// Rebuild the tiles around a changed chunk. Neighbor tiles are included
    /// because outlines depend on the chunks next to the one that changed.
    pub(crate) fn mark_chunk(&mut self, pos: ChunkPos) {
        for touched in [
            pos,
            ChunkPos::new(pos.x - 1, pos.z),
            ChunkPos::new(pos.x + 1, pos.z),
            ChunkPos::new(pos.x, pos.z - 1),
            ChunkPos::new(pos.x, pos.z + 1),
        ] {
            self.dirty.insert(tile_of(touched));
        }
    }

    /// Rebuild every tile that holds a claim.
    pub(crate) fn mark_layer(&mut self, layer: &ClaimLayer) {
        for (pos, _) in layer.iter() {
            self.dirty.insert(tile_of(pos));
        }
    }

    /// Update nation colors. Tiles are rebuilt when any color changed.
    pub(crate) fn set_colors(&mut self, colors: &BTreeMap<Uuid, [u8; 3]>) {
        if &self.colors != colors {
            self.colors.clone_from(colors);
            self.dirty.extend(self.tiles.keys().copied());
        }
    }

    /// Rebuild tiles that were marked since the last call.
    pub(crate) fn refresh(&mut self, ctx: &Context, layer: &ClaimLayer) {
        for key in std::mem::take(&mut self.dirty) {
            match build_tile(ctx, layer, &self.colors, key) {
                Some(tile) => {
                    self.tiles.insert(key, tile);
                }
                None => {
                    self.tiles.remove(&key);
                }
            }
        }
    }

    /// Draw the tint and outlines inside `canvas`.
    pub(crate) fn paint(&self, painter: &Painter, projection: &Projection, canvas: Rect) {
        let full_uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        let outlines = projection.chunk_screen_size() >= MIN_OUTLINE_CHUNK_PX;
        for (&(tile_x, tile_z), tile) in &self.tiles {
            let min = projection.chunk_to_screen(
                f64::from(tile_x * TILE_CHUNKS),
                f64::from(tile_z * TILE_CHUNKS),
            );
            let max = projection.chunk_to_screen(
                f64::from((tile_x + 1) * TILE_CHUNKS),
                f64::from((tile_z + 1) * TILE_CHUNKS),
            );
            let rect = Rect::from_min_max(min, max);
            if !rect.intersects(canvas) {
                continue;
            }
            painter.image(tile.texture.id(), rect, full_uv, Color32::WHITE);
            if outlines {
                for edge in &tile.edges {
                    paint_edge(painter, projection, canvas, edge, self.color(edge.owner));
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    fn color(&self, owner: Uuid) -> [u8; 3] {
        self.colors.get(&owner).copied().unwrap_or(ORPHAN_COLOR)
    }
}

fn tile_of(pos: ChunkPos) -> TileKey {
    (pos.x.div_euclid(TILE_CHUNKS), pos.z.div_euclid(TILE_CHUNKS))
}

fn build_tile(
    ctx: &Context,
    layer: &ClaimLayer,
    colors: &BTreeMap<Uuid, [u8; 3]>,
    key: TileKey,
) -> Option<OverlayTile> {
    let x_min = key.0 * TILE_CHUNKS;
    let z_min = key.1 * TILE_CHUNKS;
    let chunks: Vec<(ChunkPos, Uuid)> = layer
        .in_rows(z_min, z_min + TILE_CHUNKS - 1)
        .filter(|(pos, _)| pos.x >= x_min && pos.x < x_min + TILE_CHUNKS)
        .collect();
    if chunks.is_empty() {
        return None;
    }
    let side = usize::try_from(TILE_CHUNKS).unwrap_or(256);
    let mut rgba = vec![0_u8; side * side * 4];
    for (pos, owner) in &chunks {
        let [red, green, blue] = colors.get(owner).copied().unwrap_or(ORPHAN_COLOR);
        let column = usize::try_from(pos.x - x_min).unwrap_or(0);
        let row = usize::try_from(pos.z - z_min).unwrap_or(0);
        let index = (row * side + column) * 4;
        rgba[index..index + 4].copy_from_slice(&[red, green, blue, FILL_ALPHA]);
    }
    let image = ColorImage::from_rgba_unmultiplied([side, side], &rgba);
    let texture = ctx.load_texture(
        format!("map_overlay_{}_{}", key.0, key.1),
        image,
        TextureOptions::NEAREST,
    );
    Some(OverlayTile {
        texture,
        edges: compute_edges(layer, &chunks),
    })
}

/// Outline segments for `chunks`: one run per stretch of chunks whose neighbor
/// on that side has a different owner. Runs are drawn on the owner's side.
pub(crate) fn compute_edges(layer: &ClaimLayer, chunks: &[(ChunkPos, Uuid)]) -> Vec<Edge> {
    let mut raw = Vec::new();
    for &(pos, owner) in chunks {
        for side in EdgeSide::ALL {
            if layer.get(side.neighbor(pos)) != Some(owner) {
                raw.push(Edge {
                    pos,
                    side,
                    len: 1,
                    owner,
                });
            }
        }
    }
    merge_edges(raw)
}

fn merge_edges(mut raw: Vec<Edge>) -> Vec<Edge> {
    // Runs go along x for north and south edges, and along z for west and east.
    let key = |edge: &Edge| {
        let (line, along) = if edge.side.horizontal() {
            (edge.pos.z, edge.pos.x)
        } else {
            (edge.pos.x, edge.pos.z)
        };
        (edge.side, edge.owner, line, along)
    };
    raw.sort_by_key(key);
    let mut merged: Vec<Edge> = Vec::with_capacity(raw.len());
    for edge in raw {
        if let Some(last) = merged.last_mut() {
            let (last_side, last_owner, last_line, last_along) = key(last);
            let (side, owner, line, along) = key(&edge);
            if (side, owner, line) == (last_side, last_owner, last_line)
                && along == last_along + last.len
            {
                last.len += 1;
                continue;
            }
        }
        merged.push(edge);
    }
    merged
}

fn paint_edge(painter: &Painter, projection: &Projection, canvas: Rect, edge: &Edge, color: [u8; 3]) {
    let (x, z) = (f64::from(edge.pos.x), f64::from(edge.pos.z));
    let len = f64::from(edge.len);
    let half = OUTLINE_WIDTH / 2.0;
    // Endpoints in chunk corners, plus a screen-space nudge toward the owned side.
    let (start, end, inset) = match edge.side {
        EdgeSide::North => ((x, z), (x + len, z), egui::vec2(0.0, half)),
        EdgeSide::South => ((x, z + 1.0), (x + len, z + 1.0), egui::vec2(0.0, -half)),
        EdgeSide::West => ((x, z), (x, z + len), egui::vec2(half, 0.0)),
        EdgeSide::East => ((x + 1.0, z), (x + 1.0, z + len), egui::vec2(-half, 0.0)),
    };
    let from = projection.chunk_to_screen(start.0, start.1) + inset;
    let to = projection.chunk_to_screen(end.0, end.1) + inset;
    if !Rect::from_two_pos(from, to).expand(OUTLINE_WIDTH).intersects(canvas) {
        return;
    }
    let [red, green, blue] = color;
    painter.line_segment(
        [from, to],
        Stroke::new(OUTLINE_WIDTH, Color32::from_rgb(red, green, blue)),
    );
}

#[cfg(test)]
mod tests {
    use super::{compute_edges, EdgeSide, TILE_CHUNKS};
    use crate::backend::{ChunkPos, ChunkRect, ClaimLayer};
    use uuid::Uuid;

    fn square(layer: &mut ClaimLayer, owner: Uuid, from: (i32, i32), to: (i32, i32)) {
        let rect = ChunkRect::from_corners(ChunkPos::new(from.0, from.1), ChunkPos::new(to.0, to.1));
        for pos in rect.positions() {
            layer.set(pos, Some(owner));
        }
    }

    #[test]
    fn square_territory_has_four_merged_edges() {
        let owner = Uuid::from_u128(1);
        let mut layer = ClaimLayer::default();
        square(&mut layer, owner, (0, 0), (3, 2));
        let chunks: Vec<_> = layer.iter().collect();
        let edges = compute_edges(&layer, &chunks);
        assert_eq!(edges.len(), 4);
        for edge in &edges {
            match edge.side {
                EdgeSide::North | EdgeSide::South => assert_eq!(edge.len, 4),
                EdgeSide::West | EdgeSide::East => assert_eq!(edge.len, 3),
            }
        }
    }

    #[test]
    fn shared_border_is_outlined_on_both_sides() {
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let mut layer = ClaimLayer::default();
        square(&mut layer, a, (0, 0), (1, 1));
        square(&mut layer, b, (2, 0), (3, 1));
        let chunks: Vec<_> = layer.iter().collect();
        let edges = compute_edges(&layer, &chunks);
        let east_of_a = edges
            .iter()
            .filter(|edge| edge.owner == a && edge.side == EdgeSide::East)
            .count();
        let west_of_b = edges
            .iter()
            .filter(|edge| edge.owner == b && edge.side == EdgeSide::West)
            .count();
        assert_eq!((east_of_a, west_of_b), (1, 1));
    }

    #[test]
    fn hole_gets_its_own_outline() {
        let owner = Uuid::from_u128(1);
        let mut layer = ClaimLayer::default();
        square(&mut layer, owner, (0, 0), (2, 2));
        layer.set(ChunkPos::new(1, 1), None);
        let chunks: Vec<_> = layer.iter().collect();
        // Outer ring contributes 4 runs; the hole adds one run per side.
        assert_eq!(compute_edges(&layer, &chunks).len(), 8);
    }

    #[test]
    fn tiles_are_aligned_to_absolute_chunks() {
        assert_eq!(super::tile_of(ChunkPos::new(0, 0)), (0, 0));
        assert_eq!(super::tile_of(ChunkPos::new(TILE_CHUNKS - 1, 0)), (0, 0));
        assert_eq!(super::tile_of(ChunkPos::new(TILE_CHUNKS, 0)), (1, 0));
        assert_eq!(super::tile_of(ChunkPos::new(-1, -TILE_CHUNKS - 1)), (-1, -2));
    }
}
