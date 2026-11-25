//! Maps: a base image calibrated to Minecraft coordinates, plus chunk claims.
//!
//! A map lives in `maps/<slug>/` inside the project:
//!
//! ```text
//! maps/overworld/
//! ├── map.json     calibration and display settings
//! ├── base.png     the imported image
//! └── claims.json  which nation owns which chunk, plus optional block overrides
//! ```
//!
//! Claims are keyed by nation page id, so renaming a nation never touches them.
//! A chunk claim covers all 16×16 of its blocks. Block overrides cover a messy
//! border without splitting the rest of the chunk.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use uuid::Uuid;

use crate::backend::error::{MotError, Result};

pub(crate) const MAP_SCHEMA: u32 = 1;
pub(crate) const CLAIMS_SCHEMA: u32 = 1;
/// Claims files that also store per-block overrides.
const CLAIMS_SCHEMA_BLOCKS: u32 = 2;
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

/// One Minecraft block on the claim grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockPos {
    pub x: i32,
    pub z: i32,
}

impl BlockPos {
    pub(crate) const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// Fractional block positions round toward minus infinity.
    pub(crate) fn from_block_f(x: f64, z: f64) -> Self {
        Self::new(x.floor() as i32, z.floor() as i32)
    }

    pub(crate) fn chunk(self) -> ChunkPos {
        ChunkPos::new(
            self.x.div_euclid(CHUNK_BLOCKS),
            self.z.div_euclid(CHUNK_BLOCKS),
        )
    }
}

/// An inclusive rectangle of blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockRect {
    pub min: BlockPos,
    pub max: BlockPos,
}

impl BlockRect {
    pub(crate) fn from_corners(a: BlockPos, b: BlockPos) -> Self {
        Self {
            min: BlockPos::new(a.x.min(b.x), a.z.min(b.z)),
            max: BlockPos::new(a.x.max(b.x), a.z.max(b.z)),
        }
    }

    /// The 16×16 blocks that make up `chunk`.
    pub(crate) fn of_chunk(chunk: ChunkPos) -> Option<Self> {
        let x0 = chunk.x.checked_mul(CHUNK_BLOCKS)?;
        let z0 = chunk.z.checked_mul(CHUNK_BLOCKS)?;
        Some(Self {
            min: BlockPos::new(x0, z0),
            max: BlockPos::new(x0 + CHUNK_BLOCKS - 1, z0 + CHUNK_BLOCKS - 1),
        })
    }

    pub(crate) fn contains(self, pos: BlockPos) -> bool {
        pos.x >= self.min.x && pos.x <= self.max.x && pos.z >= self.min.z && pos.z <= self.max.z
    }

    pub(crate) fn contains_rect(self, other: Self) -> bool {
        self.contains(other.min) && self.contains(other.max)
    }

    /// The part of this rectangle that lies inside `bounds`, if any.
    pub(crate) fn clipped_to(self, bounds: Self) -> Option<Self> {
        let min = BlockPos::new(self.min.x.max(bounds.min.x), self.min.z.max(bounds.min.z));
        let max = BlockPos::new(self.max.x.min(bounds.max.x), self.max.z.min(bounds.max.z));
        (min.x <= max.x && min.z <= max.z).then_some(Self { min, max })
    }

    pub(crate) fn positions(self) -> impl Iterator<Item = BlockPos> {
        (self.min.z..=self.max.z)
            .flat_map(move |z| (self.min.x..=self.max.x).map(move |x| BlockPos::new(x, z)))
    }
}

/// A block whose owner differs from the chunk underneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockOverride {
    Owner(Uuid),
    /// Punch a hole in a claimed chunk.
    Clear,
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

    /// Blocks the image covers. Block painting is limited to this area.
    pub(crate) fn block_extent(&self) -> BlockRect {
        let (width, height) = (f64::from(self.image_size[0]), f64::from(self.image_size[1]));
        let (end_x, end_z) = self.pixel_to_block(width, height);
        let min = BlockPos::from_block_f(self.origin_block[0], self.origin_block[1]);
        let max = BlockPos::from_block_f(end_x - 1e-6, end_z - 1e-6);
        BlockRect {
            min,
            max: BlockPos::new(max.x.max(min.x), max.z.max(min.z)),
        }
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
///
/// `blocks` overrides individual blocks inside or outside those chunks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaimLayer {
    /// Keyed by `(z, x)` so iteration walks rows from north to south.
    owners: BTreeMap<(i32, i32), Uuid>,
    /// Keyed by block `(z, x)`.
    blocks: BTreeMap<(i32, i32), BlockOverride>,
}

/// Whole chunks and leftover blocks that a block-resolution fill should paint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRegion {
    pub chunks: Vec<ChunkPos>,
    pub blocks: Vec<BlockPos>,
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
        self.owners.is_empty() && self.blocks.is_empty()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (ChunkPos, Uuid)> + '_ {
        self.owners
            .iter()
            .map(|(&(z, x), &owner)| (ChunkPos::new(x, z), owner))
    }

    pub(crate) fn iter_blocks(&self) -> impl Iterator<Item = (BlockPos, BlockOverride)> + '_ {
        self.blocks
            .iter()
            .map(|(&(z, x), &over)| (BlockPos::new(x, z), over))
    }

    /// Claimed chunks whose row (`z`) lies in `z_min..=z_max`.
    pub(crate) fn in_rows(&self, z_min: i32, z_max: i32) -> impl Iterator<Item = (ChunkPos, Uuid)> + '_ {
        self.owners
            .range((z_min, i32::MIN)..=(z_max, i32::MAX))
            .map(|(&(z, x), &owner)| (ChunkPos::new(x, z), owner))
    }

    /// Chunks claimed by each nation, ignoring partial blocks.
    pub(crate) fn counts(&self) -> BTreeMap<Uuid, usize> {
        let mut counts = BTreeMap::new();
        for owner in self.owners.values() {
            *counts.entry(*owner).or_insert(0) += 1;
        }
        counts
    }

    /// Blocks effectively owned by each nation, counting partial chunks.
    pub(crate) fn block_area(&self) -> BTreeMap<Uuid, u64> {
        let mut area: BTreeMap<Uuid, u64> = BTreeMap::new();
        let chunk_blocks = u64::from(CHUNK_BLOCKS as u32).pow(2);
        for owner in self.owners.values() {
            *area.entry(*owner).or_insert(0) += chunk_blocks;
        }
        for (&(z, x), over) in &self.blocks {
            let base = self.get(BlockPos::new(x, z).chunk());
            match *over {
                BlockOverride::Owner(id) => {
                    if base != Some(id) {
                        if let Some(base) = base {
                            if let Some(count) = area.get_mut(&base) {
                                *count = count.saturating_sub(1);
                            }
                        }
                        *area.entry(id).or_insert(0) += 1;
                    }
                }
                BlockOverride::Clear => {
                    if let Some(base) = base {
                        if let Some(count) = area.get_mut(&base) {
                            *count = count.saturating_sub(1);
                        }
                    }
                }
            }
        }
        area.retain(|_, count| *count > 0);
        area
    }

    /// Owner of one block: an override wins, otherwise the whole chunk.
    pub(crate) fn block_owner(&self, pos: BlockPos) -> Option<Uuid> {
        match self.blocks.get(&(pos.z, pos.x)) {
            Some(BlockOverride::Owner(id)) => Some(*id),
            Some(BlockOverride::Clear) => None,
            None => self.get(pos.chunk()),
        }
    }

    pub(crate) fn block_override(&self, pos: BlockPos) -> Option<BlockOverride> {
        self.blocks.get(&(pos.z, pos.x)).copied()
    }

    /// `Some(owner)` when every block in the chunk shares that owner.
    /// `None` when block overrides break the chunk up.
    pub(crate) fn uniform_owner(&self, chunk: ChunkPos) -> Option<Option<Uuid>> {
        if self.has_block_detail(chunk) {
            None
        } else {
            Some(self.get(chunk))
        }
    }

    pub(crate) fn has_block_detail(&self, chunk: ChunkPos) -> bool {
        let Some((x0, z0)) = block_origin(chunk) else {
            return false;
        };
        let x1 = x0 + CHUNK_BLOCKS - 1;
        let z1 = z0 + CHUNK_BLOCKS - 1;
        self.blocks
            .range((z0, i32::MIN)..=(z1, i32::MAX))
            .any(|(&(_z, x), _)| x >= x0 && x <= x1)
    }

    /// Overrides inside `chunk`, optionally only the explicit holes.
    pub(crate) fn drain_overrides(
        &mut self,
        chunk: ChunkPos,
        clears_only: bool,
    ) -> Vec<(BlockPos, BlockOverride)> {
        let Some((x0, z0)) = block_origin(chunk) else {
            return Vec::new();
        };
        let x1 = x0 + CHUNK_BLOCKS - 1;
        let z1 = z0 + CHUNK_BLOCKS - 1;
        let keys: Vec<BlockPos> = self
            .blocks
            .range((z0, i32::MIN)..=(z1, i32::MAX))
            .filter(|entry| {
                let ((_, x), over) = (entry.0, entry.1);
                *x >= x0 && *x <= x1 && (!clears_only || matches!(over, BlockOverride::Clear))
            })
            .map(|(&(z, x), _)| BlockPos::new(x, z))
            .collect();
        let mut removed = Vec::with_capacity(keys.len());
        for pos in keys {
            if let Some(old) = self.blocks.remove(&(pos.z, pos.x)) {
                removed.push((pos, old));
            }
        }
        removed
    }

    pub(crate) fn write_override(&mut self, pos: BlockPos, value: Option<BlockOverride>) {
        match value {
            Some(value) => {
                self.blocks.insert((pos.z, pos.x), value);
            }
            None => {
                self.blocks.remove(&(pos.z, pos.x));
            }
        }
    }

    /// The override to store so `pos` shows `owner`, or `None` when the chunk already does.
    pub(crate) fn normalized_override(&self, pos: BlockPos, owner: Option<Uuid>) -> Option<BlockOverride> {
        if owner == self.get(pos.chunk()) {
            None
        } else if let Some(id) = owner {
            Some(BlockOverride::Owner(id))
        } else {
            Some(BlockOverride::Clear)
        }
    }

    /// Blocks connected to `start` that share its effective owner.
    ///
    /// Uniform chunks are taken whole. `None` means the region is larger than `max_cells`.
    pub(crate) fn block_region(
        &self,
        start: BlockPos,
        bounds: BlockRect,
        max_cells: usize,
    ) -> Option<BlockRegion> {
        if !bounds.contains(start) {
            return Some(BlockRegion {
                chunks: Vec::new(),
                blocks: Vec::new(),
            });
        }
        let target = self.block_owner(start);
        let mut chunks = Vec::new();
        let mut blocks = Vec::new();
        let mut seen_chunks = HashSet::new();
        let mut seen_blocks = HashSet::new();
        let mut stack = vec![FillTask::Block(start)];
        while let Some(task) = stack.pop() {
            match task {
                FillTask::Chunk(chunk) => {
                    if seen_chunks.contains(&chunk) {
                        continue;
                    }
                    let Some(chunk_rect) = BlockRect::of_chunk(chunk) else {
                        continue;
                    };
                    if self.uniform_owner(chunk) != Some(target) || !bounds.contains_rect(chunk_rect) {
                        continue;
                    }
                    seen_chunks.insert(chunk);
                    if chunks.len() >= max_cells {
                        return None;
                    }
                    chunks.push(chunk);
                    for side in 0..4 {
                        let neighbor = chunk_step(chunk, side);
                        let neighbor_rect = BlockRect::of_chunk(neighbor);
                        let solid = self.uniform_owner(neighbor) == Some(target);
                        if solid && neighbor_rect.is_some_and(|rect| bounds.contains_rect(rect)) {
                            stack.push(FillTask::Chunk(neighbor));
                        } else if let Some(rect) = BlockRect::of_chunk(chunk) {
                            for pos in edge_outside(rect, side) {
                                stack.push(FillTask::Block(pos));
                            }
                        }
                    }
                }
                FillTask::Block(pos) => {
                    if !bounds.contains(pos) || !seen_blocks.insert(pos) {
                        continue;
                    }
                    if self.block_owner(pos) != target {
                        continue;
                    }
                    let chunk = pos.chunk();
                    if seen_chunks.contains(&chunk) {
                        continue;
                    }
                    if self.uniform_owner(chunk) == Some(target)
                        && BlockRect::of_chunk(chunk).is_some_and(|rect| bounds.contains_rect(rect))
                    {
                        stack.push(FillTask::Chunk(chunk));
                        continue;
                    }
                    if blocks.len() >= max_cells {
                        return None;
                    }
                    blocks.push(pos);
                    for side in 0..4 {
                        stack.push(FillTask::Block(block_step(pos, side)));
                    }
                }
            }
        }
        Some(BlockRegion { chunks, blocks })
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
        let mut block_runs: BTreeMap<Uuid, Vec<(i32, i32)>> = BTreeMap::new();
        let mut clears = Vec::new();
        for (&(z, x), over) in &self.blocks {
            match *over {
                BlockOverride::Owner(id) => block_runs.entry(id).or_default().push((z, x)),
                BlockOverride::Clear => clears.push((z, x)),
            }
        }
        let file = ClaimsFile {
            schema: if self.blocks.is_empty() {
                CLAIMS_SCHEMA
            } else {
                CLAIMS_SCHEMA_BLOCKS
            },
            claims: encode_groups(group_runs(&self.owners)),
            blocks: encode_groups(block_runs),
            clears: encode_runs(clears),
        };
        Ok(serde_json::to_vec_pretty(&file)?)
    }

    /// Read the `claims.json` text.
    pub(crate) fn from_json(bytes: &[u8]) -> Result<Self> {
        let file: ClaimsFile = serde_json::from_slice(bytes)?;
        if file.schema != CLAIMS_SCHEMA && file.schema != CLAIMS_SCHEMA_BLOCKS {
            return Err(MotError::new(format!(
                "unsupported claims schema {}",
                file.schema
            )));
        }
        let mut layer = Self::default();
        for claim in file.claims {
            for pos in decode_runs(&claim.runs)? {
                layer.set(pos, Some(claim.nation));
            }
        }
        for claim in file.blocks {
            for pos in decode_runs(&claim.runs)? {
                layer.write_override(
                    BlockPos::new(pos.x, pos.z),
                    Some(BlockOverride::Owner(claim.nation)),
                );
            }
        }
        for pos in decode_runs(&file.clears)? {
            layer.write_override(BlockPos::new(pos.x, pos.z), Some(BlockOverride::Clear));
        }
        Ok(layer)
    }
}

fn block_origin(chunk: ChunkPos) -> Option<(i32, i32)> {
    Some((
        chunk.x.checked_mul(CHUNK_BLOCKS)?,
        chunk.z.checked_mul(CHUNK_BLOCKS)?,
    ))
}

enum FillTask {
    Chunk(ChunkPos),
    Block(BlockPos),
}

fn chunk_step(pos: ChunkPos, side: usize) -> ChunkPos {
    match side {
        0 => ChunkPos::new(pos.x, pos.z - 1),
        1 => ChunkPos::new(pos.x, pos.z + 1),
        2 => ChunkPos::new(pos.x - 1, pos.z),
        _ => ChunkPos::new(pos.x + 1, pos.z),
    }
}

fn block_step(pos: BlockPos, side: usize) -> BlockPos {
    match side {
        0 => BlockPos::new(pos.x, pos.z - 1),
        1 => BlockPos::new(pos.x, pos.z + 1),
        2 => BlockPos::new(pos.x - 1, pos.z),
        _ => BlockPos::new(pos.x + 1, pos.z),
    }
}

/// Blocks just outside one side of `rect`. Sides are north, south, west, east.
fn edge_outside(rect: BlockRect, side: usize) -> impl Iterator<Item = BlockPos> {
    let (min, max) = (rect.min, rect.max);
    let (fixed, span) = match side {
        0 => (min.z - 1, (min.x, max.x)),
        1 => (max.z + 1, (min.x, max.x)),
        2 => (min.x - 1, (min.z, max.z)),
        _ => (max.x + 1, (min.z, max.z)),
    };
    let horizontal = side < 2;
    (span.0..=span.1).map(move |along| {
        if horizontal {
            BlockPos::new(along, fixed)
        } else {
            BlockPos::new(fixed, along)
        }
    })
}

fn group_runs(owners: &BTreeMap<(i32, i32), Uuid>) -> BTreeMap<Uuid, Vec<(i32, i32)>> {
    let mut grouped: BTreeMap<Uuid, Vec<(i32, i32)>> = BTreeMap::new();
    for (&(z, x), owner) in owners {
        grouped.entry(*owner).or_default().push((z, x));
    }
    grouped
}

fn encode_groups(groups: BTreeMap<Uuid, Vec<(i32, i32)>>) -> Vec<NationClaims> {
    groups
        .into_iter()
        .map(|(nation, points)| NationClaims {
            nation,
            runs: encode_runs(points),
        })
        .collect()
}

fn encode_runs(points: Vec<(i32, i32)>) -> Vec<[i32; 3]> {
    let mut runs: Vec<[i32; 3]> = Vec::new();
    for (z, x) in points {
        match runs.last_mut() {
            Some(last) if last[0] == z && last[2].checked_add(1) == Some(x) => last[2] = x,
            _ => runs.push([z, x, x]),
        }
    }
    runs
}

fn decode_runs(runs: &[[i32; 3]]) -> Result<Vec<ChunkPos>> {
    let mut positions = Vec::new();
    for [z, x_start, x_end] in runs {
        if x_end < x_start {
            return Err(MotError::new("claims contain a run that ends before it starts"));
        }
        for x in *x_start..=*x_end {
            positions.push(ChunkPos::new(x, *z));
        }
    }
    Ok(positions)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ClaimsFile {
    schema: u32,
    #[serde(default)]
    claims: Vec<NationClaims>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    blocks: Vec<NationClaims>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    clears: Vec<[i32; 3]>,
}

/// One nation's chunks as `[z, x_start, x_end]` rows.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct NationClaims {
    nation: Uuid,
    runs: Vec<[i32; 3]>,
}

/// One stored block edit, kept so a stroke can be undone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockChange {
    pub pos: BlockPos,
    pub old: Option<BlockOverride>,
    pub new: Option<BlockOverride>,
}

/// A chunk edit or a block override edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimEdit {
    Chunk(ClaimChange),
    Block(BlockChange),
}

/// A claim layer with undo and redo. Edits are grouped into strokes.
#[derive(Debug, Clone, Default)]
pub struct ClaimEditor {
    layer: ClaimLayer,
    undo: Vec<Vec<ClaimEdit>>,
    redo: Vec<Vec<ClaimEdit>>,
    pending: Vec<ClaimEdit>,
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
    ///
    /// Painting a chunk also drops the overrides that no longer apply, so the
    /// chunk becomes uniform again.
    pub(crate) fn paint(&mut self, pos: ChunkPos, owner: Option<Uuid>) -> bool {
        let old = self.layer.get(pos);
        let removed = self.layer.drain_overrides(pos, owner.is_none());
        if old == owner && removed.is_empty() {
            return false;
        }
        self.layer.set(pos, owner);
        self.pending.push(ClaimEdit::Chunk(ClaimChange {
            pos,
            old,
            new: owner,
        }));
        for (block, previous) in removed {
            self.pending.push(ClaimEdit::Block(BlockChange {
                pos: block,
                old: Some(previous),
                new: None,
            }));
        }
        true
    }

    /// Change one block as part of the current stroke. Returns whether anything changed.
    pub(crate) fn paint_block(&mut self, pos: BlockPos, owner: Option<Uuid>) -> bool {
        let old = self.layer.block_override(pos);
        let new = self.layer.normalized_override(pos, owner);
        if old == new {
            return false;
        }
        self.layer.write_override(pos, new);
        self.pending.push(ClaimEdit::Block(BlockChange { pos, old, new }));
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

    /// Remove every chunk and block owned by `owner` as a single undo step.
    pub(crate) fn clear_owner(&mut self, owner: Uuid) -> Vec<ClaimEdit> {
        let chunks: Vec<ChunkPos> = self
            .layer
            .iter()
            .filter(|(_, id)| *id == owner)
            .map(|(pos, _)| pos)
            .collect();
        let blocks: Vec<BlockPos> = self
            .layer
            .blocks
            .iter()
            .filter_map(|(&(z, x), over)| match over {
                BlockOverride::Owner(id) if *id == owner => Some(BlockPos::new(x, z)),
                _ => None,
            })
            .collect();
        for pos in chunks {
            self.paint(pos, None);
        }
        for pos in blocks {
            let reveal = self.layer.get(pos.chunk());
            self.paint_block(pos, reveal);
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

    /// Revert the last stroke and return the edits that changed.
    pub(crate) fn undo(&mut self) -> Vec<ClaimEdit> {
        self.commit();
        let Some(stroke) = self.undo.pop() else {
            return Vec::new();
        };
        for change in stroke.iter().rev() {
            self.apply_reverse(change);
        }
        self.redo.push(stroke.clone());
        stroke
    }

    /// Reapply the last undone stroke and return the edits that changed.
    pub(crate) fn redo(&mut self) -> Vec<ClaimEdit> {
        let Some(stroke) = self.redo.pop() else {
            return Vec::new();
        };
        for change in &stroke {
            self.apply_forward(change);
        }
        self.undo.push(stroke.clone());
        stroke
    }

    fn apply_forward(&mut self, change: &ClaimEdit) {
        match *change {
            ClaimEdit::Chunk(change) => {
                self.layer.set(change.pos, change.new);
            }
            ClaimEdit::Block(change) => self.layer.write_override(change.pos, change.new),
        }
    }

    fn apply_reverse(&mut self, change: &ClaimEdit) {
        match *change {
            ClaimEdit::Chunk(change) => {
                self.layer.set(change.pos, change.old);
            }
            ClaimEdit::Block(change) => self.layer.write_override(change.pos, change.old),
        }
    }
}

/// "N chunks, M blocks²", with a remainder when the area is not a whole number of chunks.
pub(crate) fn format_claim_area(blocks: u64) -> String {
    let per = u64::from(CHUNK_BLOCKS as u32).pow(2);
    let chunks = blocks / per;
    let extra = blocks % per;
    if extra == 0 {
        format!("{chunks} chunks, {blocks} blocks²")
    } else if chunks == 0 {
        format!("{extra} blocks²")
    } else {
        format!("{chunks} chunks + {extra} blocks, {blocks} blocks²")
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
    /// Whole chunks claimed by each nation.
    pub counts: BTreeMap<Uuid, usize>,
    /// Effective blocks claimed by each nation, including partial chunks.
    pub block_area: BTreeMap<Uuid, u64>,
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
    use super::{
        BlockOverride, BlockPos, BlockRect, ChunkPos, ChunkRect, ClaimEditor, ClaimLayer, MapMeta,
        CHUNK_BLOCKS, MAP_SCHEMA,
    };
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

    #[test]
    fn block_override_wins_over_the_chunk_and_round_trips() {
        let chunk_owner = Uuid::from_u128(1);
        let border = Uuid::from_u128(2);
        let mut layer = ClaimLayer::default();
        layer.set(ChunkPos::new(0, 0), Some(chunk_owner));
        let hole = BlockPos::new(1, 2);
        let foreign = BlockPos::new(4, 4);
        let mut editor = ClaimEditor::new(layer);
        assert!(editor.paint_block(hole, None));
        assert!(editor.paint_block(foreign, Some(border)));
        assert!(!editor.paint_block(BlockPos::new(0, 0), Some(chunk_owner)), "matching the chunk stores nothing");
        editor.commit();
        let layer = editor.layer();
        assert_eq!(layer.block_owner(hole), None);
        assert_eq!(layer.block_owner(foreign), Some(border));
        assert_eq!(layer.block_owner(BlockPos::new(0, 0)), Some(chunk_owner));
        assert!(layer.has_block_detail(ChunkPos::new(0, 0)));
        let decoded = ClaimLayer::from_json(&layer.to_json().expect("encode")).expect("decode");
        assert_eq!(decoded, *layer);
        let area = decoded.block_area();
        assert_eq!(area.get(&chunk_owner), Some(&(256 - 2)));
        assert_eq!(area.get(&border), Some(&1));
    }

    #[test]
    fn painting_a_chunk_clears_overrides_inside_it() {
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let mut editor = ClaimEditor::default();
        editor.paint_block(BlockPos::new(3, 3), Some(a));
        editor.paint_block(BlockPos::new(-1, -1), Some(a));
        editor.commit();
        editor.paint(ChunkPos::new(0, 0), Some(b));
        editor.commit();
        assert_eq!(editor.layer().block_owner(BlockPos::new(3, 3)), Some(b));
        assert!(!editor.layer().has_block_detail(ChunkPos::new(0, 0)));
        assert_eq!(editor.layer().block_owner(BlockPos::new(-1, -1)), Some(a));
        editor.undo();
        assert_eq!(editor.layer().block_owner(BlockPos::new(3, 3)), Some(a));
    }

    #[test]
    fn fill_inside_a_block_border_stops_at_the_border() {
        let nation = Uuid::from_u128(1);
        let mut layer = ClaimLayer::default();
        for x in 0..=4 {
            for z in 0..=4 {
                if x == 0 || x == 4 || z == 0 || z == 4 {
                    layer.write_override(BlockPos::new(x, z), Some(BlockOverride::Owner(nation)));
                }
            }
        }
        let bounds = BlockRect::from_corners(BlockPos::new(-32, -32), BlockPos::new(64, 64));
        let region = layer.block_region(BlockPos::new(2, 2), bounds, 10_000).expect("small");
        assert!(region.chunks.is_empty(), "the interior is not a whole chunk");
        assert_eq!(region.blocks.len(), 9);
        assert!(region.blocks.contains(&BlockPos::new(1, 1)));
        assert!(!region.blocks.contains(&BlockPos::new(0, 0)));
        assert!(!region.blocks.contains(&BlockPos::new(5, 2)));
    }

    #[test]
    fn fill_of_a_uniform_chunk_does_not_cross_a_block_border() {
        let nation = Uuid::from_u128(1);
        let mut layer = ClaimLayer::default();
        // A one-block wall around chunk (0, 0), whose blocks are 0..16 on each axis.
        let last = CHUNK_BLOCKS - 1;
        for along in 0..=last {
            for pos in [
                BlockPos::new(-1, along),
                BlockPos::new(CHUNK_BLOCKS, along),
                BlockPos::new(along, -1),
                BlockPos::new(along, CHUNK_BLOCKS),
            ] {
                layer.write_override(pos, Some(BlockOverride::Owner(nation)));
            }
        }
        let bounds = BlockRect::from_corners(BlockPos::new(-32, -32), BlockPos::new(64, 64));
        let region = layer
            .block_region(BlockPos::new(1, 1), bounds, 10_000)
            .expect("small");
        assert_eq!(region.chunks, vec![ChunkPos::new(0, 0)]);
        assert!(region.blocks.is_empty());
    }
}
