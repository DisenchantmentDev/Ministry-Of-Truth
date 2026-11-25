//! Maps: a base image calibrated to Minecraft coordinates, plus chunk claims.
//!
//! A map lives in `maps/<slug>/` inside the project:
//!
//! ```text
//! maps/overworld/
//! ├── map.json     calibration and display settings
//! ├── base.png     the imported image
//! └── claims.json  which nation owns which chunk
//! ```
//!
//! Claims are keyed by nation page id, so renaming a nation never touches them.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use uuid::Uuid;

use crate::backend::error::{MotError, Result};

pub(crate) const MAP_SCHEMA: u32 = 1;
pub(crate) const CLAIMS_SCHEMA: u32 = 1;
/// Blocks along one side of a chunk.
pub(crate) const CHUNK_BLOCKS: i32 = 16;
/// How many undo steps are kept per open map.
const HISTORY_LIMIT: usize = 100;

pub(crate) const MAP_FILE: &str = "map.json";
pub(crate) const CLAIMS_FILE: &str = "claims.json";

/// A chunk coordinate. Minecraft chunks are 16x16 blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub(crate) const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// The chunk that contains a fractional block position. Negative blocks
    /// round toward minus infinity.
    pub(crate) fn from_block_f(x: f64, z: f64) -> Self {
        let size = f64::from(CHUNK_BLOCKS);
        Self::new((x / size).floor() as i32, (z / size).floor() as i32)
    }
}

/// An inclusive rectangle of chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkRect {
    pub min: ChunkPos,
    pub max: ChunkPos,
}

impl ChunkRect {
    /// The rectangle spanned by two corners, in either order.
    pub(crate) fn from_corners(a: ChunkPos, b: ChunkPos) -> Self {
        Self {
            min: ChunkPos::new(a.x.min(b.x), a.z.min(b.z)),
            max: ChunkPos::new(a.x.max(b.x), a.z.max(b.z)),
        }
    }

    pub(crate) fn contains(self, pos: ChunkPos) -> bool {
        pos.x >= self.min.x && pos.x <= self.max.x && pos.z >= self.min.z && pos.z <= self.max.z
    }

    /// Every position inside the rectangle, row by row.
    pub(crate) fn positions(self) -> impl Iterator<Item = ChunkPos> {
        (self.min.z..=self.max.z)
            .flat_map(move |z| (self.min.x..=self.max.x).map(move |x| ChunkPos::new(x, z)))
    }

    /// The part of this rectangle that lies inside `bounds`, if any.
    pub(crate) fn clipped_to(self, bounds: Self) -> Option<Self> {
        let min = ChunkPos::new(self.min.x.max(bounds.min.x), self.min.z.max(bounds.min.z));
        let max = ChunkPos::new(self.max.x.min(bounds.max.x), self.max.z.min(bounds.max.z));
        (min.x <= max.x && min.z <= max.z).then_some(Self { min, max })
    }
}

/// Calibration and display settings stored in `map.json`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MapMeta {
    pub schema: u32,
    pub name: String,
    /// File name of the base image inside the map folder.
    pub image: String,
    /// Width and height of the base image in pixels.
    pub image_size: [u32; 2],
    /// Image pixels per Minecraft block.
    pub px_per_block: f64,
    /// Block coordinates (x, z) of the image's top-left corner.
    pub origin_block: [f64; 2],
    /// Informational label such as "Overworld".
    #[serde(default)]
    pub dimension: String,
}

impl MapMeta {
    /// Block position under an image pixel.
    pub(crate) fn pixel_to_block(&self, px: f64, py: f64) -> (f64, f64) {
        (
            self.origin_block[0] + px / self.px_per_block,
            self.origin_block[1] + py / self.px_per_block,
        )
    }

    /// Chunks that overlap the image. Painting is limited to this area.
    pub(crate) fn chunk_extent(&self) -> ChunkRect {
        let (width, height) = (f64::from(self.image_size[0]), f64::from(self.image_size[1]));
        let (end_x, end_z) = self.pixel_to_block(width, height);
        let min = ChunkPos::from_block_f(self.origin_block[0], self.origin_block[1]);
        // The image ends just before `end`, so the last chunk is the one holding `end - epsilon`.
        let max = ChunkPos::from_block_f(end_x - 1e-6, end_z - 1e-6);
        ChunkRect {
            min,
            max: ChunkPos::new(max.x.max(min.x), max.z.max(min.z)),
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if !(self.px_per_block.is_finite() && self.px_per_block > 0.0) {
            return Err(MotError::new("pixels per block must be a positive number"));
        }
        if !self.origin_block.iter().all(|value| value.is_finite()) {
            return Err(MotError::new("origin coordinates must be numbers"));
        }
        if self.image_size[0] == 0 || self.image_size[1] == 0 {
            return Err(MotError::new("the map image is empty"));
        }
        Ok(())
    }
}

/// One chunk edit, kept so it can be undone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaimChange {
    pub pos: ChunkPos,
    pub old: Option<Uuid>,
    pub new: Option<Uuid>,
}

/// Which nation owns each chunk. A chunk has at most one owner.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaimLayer {
    /// Keyed by `(z, x)` so iteration walks rows from north to south.
    owners: BTreeMap<(i32, i32), Uuid>,
}

impl ClaimLayer {
    pub(crate) fn get(&self, pos: ChunkPos) -> Option<Uuid> {
        self.owners.get(&(pos.z, pos.x)).copied()
    }

    /// Set a chunk's owner and return the previous owner.
    pub(crate) fn set(&mut self, pos: ChunkPos, owner: Option<Uuid>) -> Option<Uuid> {
        match owner {
            Some(id) => self.owners.insert((pos.z, pos.x), id),
            None => self.owners.remove(&(pos.z, pos.x)),
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.owners.len()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (ChunkPos, Uuid)> + '_ {
        self.owners
            .iter()
            .map(|(&(z, x), &owner)| (ChunkPos::new(x, z), owner))
    }

    /// Claimed chunks whose row (`z`) lies in `z_min..=z_max`.
    pub(crate) fn in_rows(&self, z_min: i32, z_max: i32) -> impl Iterator<Item = (ChunkPos, Uuid)> + '_ {
        self.owners
            .range((z_min, i32::MIN)..=(z_max, i32::MAX))
            .map(|(&(z, x), &owner)| (ChunkPos::new(x, z), owner))
    }

    /// Chunks claimed by each nation.
    pub(crate) fn counts(&self) -> BTreeMap<Uuid, usize> {
        let mut counts = BTreeMap::new();
        for owner in self.owners.values() {
            *counts.entry(*owner).or_insert(0) += 1;
        }
        counts
    }

    /// Chunks connected to `start` (by edges) that share its owner, inside `bounds`.
    pub(crate) fn connected_region(&self, start: ChunkPos, bounds: ChunkRect) -> Vec<ChunkPos> {
        if !bounds.contains(start) {
            return Vec::new();
        }
        let target = self.get(start);
        let mut seen = HashSet::new();
        let mut stack = vec![start];
        let mut region = Vec::new();
        seen.insert(start);
        while let Some(pos) = stack.pop() {
            region.push(pos);
            for next in [
                ChunkPos::new(pos.x - 1, pos.z),
                ChunkPos::new(pos.x + 1, pos.z),
                ChunkPos::new(pos.x, pos.z - 1),
                ChunkPos::new(pos.x, pos.z + 1),
            ] {
                if bounds.contains(next) && self.get(next) == target && seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        region
    }

    /// Serialize to the `claims.json` text.
    pub(crate) fn to_json(&self) -> Result<Vec<u8>> {
        let mut runs: BTreeMap<Uuid, Vec<[i32; 3]>> = BTreeMap::new();
        for (&(z, x), owner) in &self.owners {
            let owner_runs = runs.entry(*owner).or_default();
            match owner_runs.last_mut() {
                Some(last) if last[0] == z && last[2].checked_add(1) == Some(x) => last[2] = x,
                _ => owner_runs.push([z, x, x]),
            }
        }
        let file = ClaimsFile {
            schema: CLAIMS_SCHEMA,
            claims: runs
                .into_iter()
                .map(|(nation, runs)| NationClaims { nation, runs })
                .collect(),
        };
        Ok(serde_json::to_vec_pretty(&file)?)
    }

    /// Read the `claims.json` text.
    pub(crate) fn from_json(bytes: &[u8]) -> Result<Self> {
        let file: ClaimsFile = serde_json::from_slice(bytes)?;
        if file.schema != CLAIMS_SCHEMA {
            return Err(MotError::new(format!(
                "unsupported claims schema {}",
                file.schema
            )));
        }
        let mut layer = Self::default();
        for claim in file.claims {
            for [z, x_start, x_end] in claim.runs {
                if x_end < x_start {
                    return Err(MotError::new("claims contain a run that ends before it starts"));
                }
                for x in x_start..=x_end {
                    layer.set(ChunkPos::new(x, z), Some(claim.nation));
                }
            }
        }
        Ok(layer)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ClaimsFile {
    schema: u32,
    #[serde(default)]
    claims: Vec<NationClaims>,
}

/// One nation's chunks as `[z, x_start, x_end]` rows.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct NationClaims {
    nation: Uuid,
    runs: Vec<[i32; 3]>,
}

/// A claim layer with undo and redo. Edits are grouped into strokes.
#[derive(Debug, Clone, Default)]
pub struct ClaimEditor {
    layer: ClaimLayer,
    undo: Vec<Vec<ClaimChange>>,
    redo: Vec<Vec<ClaimChange>>,
    pending: Vec<ClaimChange>,
}

impl ClaimEditor {
    pub(crate) fn new(layer: ClaimLayer) -> Self {
        Self {
            layer,
            ..Self::default()
        }
    }

    pub(crate) fn layer(&self) -> &ClaimLayer {
        &self.layer
    }

    /// Change one chunk as part of the current stroke. Returns whether anything changed.
    pub(crate) fn paint(&mut self, pos: ChunkPos, owner: Option<Uuid>) -> bool {
        let old = self.layer.get(pos);
        if old == owner {
            return false;
        }
        self.layer.set(pos, owner);
        self.pending.push(ClaimChange {
            pos,
            old,
            new: owner,
        });
        true
    }

    /// Finish the current stroke so it becomes one undo step.
    pub(crate) fn commit(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        self.undo.push(std::mem::take(&mut self.pending));
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Remove every chunk owned by `owner` as a single undo step.
    pub(crate) fn clear_owner(&mut self, owner: Uuid) -> Vec<ClaimChange> {
        let doomed: Vec<ChunkPos> = self
            .layer
            .iter()
            .filter(|(_, id)| *id == owner)
            .map(|(pos, _)| pos)
            .collect();
        for pos in &doomed {
            self.paint(*pos, None);
        }
        let changes = self.pending.clone();
        self.commit();
        changes
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Revert the last stroke and return the chunks that changed.
    pub(crate) fn undo(&mut self) -> Vec<ClaimChange> {
        self.commit();
        let Some(stroke) = self.undo.pop() else {
            return Vec::new();
        };
        for change in stroke.iter().rev() {
            self.layer.set(change.pos, change.old);
        }
        self.redo.push(stroke.clone());
        stroke
    }

    /// Reapply the last undone stroke and return the chunks that changed.
    pub(crate) fn redo(&mut self) -> Vec<ClaimChange> {
        let Some(stroke) = self.redo.pop() else {
            return Vec::new();
        };
        for change in &stroke {
            self.layer.set(change.pos, change.new);
        }
        self.undo.push(stroke.clone());
        stroke
    }
}

/// A map loaded from disk.
#[derive(Debug, Clone)]
pub struct MapDoc {
    pub slug: String,
    pub meta: MapMeta,
    pub claims: ClaimLayer,
}

/// Listing entry for a map in the project, with per-nation chunk counts.
#[derive(Debug, Clone)]
pub struct MapSummary {
    pub slug: String,
    pub meta: MapMeta,
    pub counts: BTreeMap<Uuid, usize>,
}

pub(crate) fn read_meta(dir: &Path) -> Result<MapMeta> {
    let text = fs::read_to_string(dir.join(MAP_FILE))?;
    let meta: MapMeta = serde_json::from_str(&text)?;
    if meta.schema != MAP_SCHEMA {
        return Err(MotError::new(format!(
            "unsupported map schema {}",
            meta.schema
        )));
    }
    Ok(meta)
}

pub(crate) fn read_claims(dir: &Path) -> Result<ClaimLayer> {
    let path = dir.join(CLAIMS_FILE);
    if !path.exists() {
        return Ok(ClaimLayer::default());
    }
    ClaimLayer::from_json(&fs::read(path)?)
}

pub(crate) fn write_meta(dir: &Path, meta: &MapMeta) -> Result<()> {
    fs::write(dir.join(MAP_FILE), serde_json::to_vec_pretty(meta)?)?;
    Ok(())
}

pub(crate) fn write_claims(dir: &Path, claims: &ClaimLayer) -> Result<()> {
    fs::write(dir.join(CLAIMS_FILE), claims.to_json()?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ChunkPos, ChunkRect, ClaimEditor, ClaimLayer, MapMeta, MAP_SCHEMA};
    use uuid::Uuid;

    fn meta(px_per_block: f64, origin: [f64; 2], size: [u32; 2]) -> MapMeta {
        MapMeta {
            schema: MAP_SCHEMA,
            name: "Test".to_owned(),
            image: "base.png".to_owned(),
            image_size: size,
            px_per_block,
            origin_block: origin,
            dimension: String::new(),
        }
    }

    #[test]
    fn negative_blocks_map_to_the_chunk_below() {
        assert_eq!(ChunkPos::from_block_f(0.0, 0.0), ChunkPos::new(0, 0));
        assert_eq!(ChunkPos::from_block_f(15.9, 16.0), ChunkPos::new(0, 1));
        assert_eq!(ChunkPos::from_block_f(-1.0, -16.0), ChunkPos::new(-1, -1));
        assert_eq!(ChunkPos::from_block_f(-17.0, -32.0), ChunkPos::new(-2, -2));
        assert_eq!(ChunkPos::from_block_f(-0.5, 31.9), ChunkPos::new(-1, 1));
    }

    #[test]
    fn pixel_converts_to_block() {
        let map = meta(0.5, [-1000.0, -500.0], [400, 200]);
        let (x, z) = map.pixel_to_block(100.0, 40.0);
        assert!((x - -800.0).abs() < 1e-9 && (z - -420.0).abs() < 1e-9);
    }

    #[test]
    fn extent_covers_every_chunk_the_image_touches() {
        // 256 px at 1 px/block starting at block -8 spans blocks -8..248.
        let map = meta(1.0, [-8.0, 0.0], [256, 32]);
        let extent = map.chunk_extent();
        assert_eq!(extent.min, ChunkPos::new(-1, 0));
        assert_eq!(extent.max, ChunkPos::new(15, 1));
        // An image ending exactly on a chunk border does not claim the next chunk.
        let aligned = meta(1.0, [0.0, 0.0], [32, 32]);
        assert_eq!(aligned.chunk_extent().max, ChunkPos::new(1, 1));
    }

    #[test]
    fn validate_rejects_bad_scale() {
        assert!(meta(0.0, [0.0, 0.0], [10, 10]).validate().is_err());
        assert!(meta(f64::NAN, [0.0, 0.0], [10, 10]).validate().is_err());
        assert!(meta(2.0, [0.0, 0.0], [10, 10]).validate().is_ok());
    }

    #[test]
    fn claims_round_trip_through_runs() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let mut layer = ClaimLayer::default();
        for x in -3..=4 {
            layer.set(ChunkPos::new(x, 2), Some(a));
        }
        layer.set(ChunkPos::new(10, 2), Some(a));
        layer.set(ChunkPos::new(0, -5), Some(b));
        layer.set(ChunkPos::new(1, -5), Some(b));
        layer.set(ChunkPos::new(0, 3), Some(b));
        let bytes = layer.to_json().expect("encode");
        let decoded = ClaimLayer::from_json(&bytes).expect("decode");
        assert_eq!(decoded, layer);
        assert_eq!(decoded.len(), 12);
        assert_eq!(decoded.counts().get(&a), Some(&9));
    }

    #[test]
    fn interleaved_owners_do_not_merge_runs() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let mut layer = ClaimLayer::default();
        layer.set(ChunkPos::new(0, 0), Some(a));
        layer.set(ChunkPos::new(1, 0), Some(b));
        layer.set(ChunkPos::new(2, 0), Some(a));
        let decoded = ClaimLayer::from_json(&layer.to_json().expect("encode")).expect("decode");
        assert_eq!(decoded, layer);
        assert_eq!(decoded.get(ChunkPos::new(1, 0)), Some(b));
    }

    #[test]
    fn reversed_run_is_an_error() {
        let text = r#"{"schema":1,"claims":[{"nation":"00000000-0000-0000-0000-000000000001","runs":[[0,5,2]]}]}"#;
        assert!(ClaimLayer::from_json(text.as_bytes()).is_err());
    }

    #[test]
    fn flood_fill_stays_inside_bounds_and_owner() {
        let a = Uuid::from_u128(1);
        let mut layer = ClaimLayer::default();
        // A 3x3 block of `a` with a hole in the middle, inside a 5x5 field.
        for pos in ChunkRect::from_corners(ChunkPos::new(1, 1), ChunkPos::new(3, 3)).positions() {
            layer.set(pos, Some(a));
        }
        layer.set(ChunkPos::new(2, 2), None);
        let bounds = ChunkRect::from_corners(ChunkPos::new(0, 0), ChunkPos::new(4, 4));
        let ring = layer.connected_region(ChunkPos::new(1, 1), bounds);
        assert_eq!(ring.len(), 8);
        let hole = layer.connected_region(ChunkPos::new(2, 2), bounds);
        assert_eq!(hole, vec![ChunkPos::new(2, 2)]);
        let outside = layer.connected_region(ChunkPos::new(0, 0), bounds);
        assert_eq!(outside.len(), 25 - 9);
        assert!(layer.connected_region(ChunkPos::new(9, 9), bounds).is_empty());
    }

    #[test]
    fn undo_and_redo_restore_strokes() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let mut editor = ClaimEditor::default();
        assert!(editor.paint(ChunkPos::new(0, 0), Some(a)));
        assert!(editor.paint(ChunkPos::new(1, 0), Some(a)));
        assert!(!editor.paint(ChunkPos::new(1, 0), Some(a)), "no-op edits are skipped");
        editor.commit();
        editor.paint(ChunkPos::new(1, 0), Some(b));
        editor.paint(ChunkPos::new(0, 0), None);
        editor.commit();
        assert_eq!(editor.layer().get(ChunkPos::new(1, 0)), Some(b));
        assert_eq!(editor.layer().get(ChunkPos::new(0, 0)), None);

        let reverted = editor.undo();
        assert_eq!(reverted.len(), 2);
        assert_eq!(editor.layer().get(ChunkPos::new(1, 0)), Some(a));
        assert_eq!(editor.layer().get(ChunkPos::new(0, 0)), Some(a));

        editor.undo();
        assert!(editor.layer().is_empty());
        assert!(!editor.can_undo() && editor.can_redo());

        editor.redo();
        editor.redo();
        assert_eq!(editor.layer().get(ChunkPos::new(1, 0)), Some(b));
        assert_eq!(editor.layer().get(ChunkPos::new(0, 0)), None);

        editor.undo();
        editor.paint(ChunkPos::new(5, 5), Some(b));
        editor.commit();
        assert!(!editor.can_redo(), "a new stroke drops the redo history");
    }

    #[test]
    fn clear_owner_is_one_undo_step() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let mut editor = ClaimEditor::default();
        for x in 0..4 {
            editor.paint(ChunkPos::new(x, 0), Some(if x < 3 { a } else { b }));
        }
        editor.commit();
        assert_eq!(editor.clear_owner(a).len(), 3);
        assert_eq!(editor.layer().len(), 1);
        editor.undo();
        assert_eq!(editor.layer().len(), 4);
    }
}
