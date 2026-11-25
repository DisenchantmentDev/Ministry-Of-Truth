use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use uuid::Uuid;

use crate::backend::buffers::TextBuf;
use crate::backend::error::{MotError, Result};
use crate::backend::links::{resolve_link, rewrite_title_fields, rewrite_wiki_links};
use crate::backend::map::{
    read_claims, read_meta, write_claims, write_meta, ClaimLayer, MapDoc, MapMeta, MapSummary,
    MAP_SCHEMA,
};
use crate::backend::page::{Frontmatter, Page, PageData};

const SCHEMA: u32 = 1;
const MAPS_DIR: &str = "maps";
const MAP_IMAGE_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

pub(crate) fn parse_start_date(text: &str) -> Result<Option<NaiveDate>> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map(Some)
        .map_err(|_| MotError::new("Day 0 must be YYYY-MM-DD"))
}

/// Top-level event metadata stored in `project.json`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectMeta {
    pub schema: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<NaiveDate>,
}

/// Frontmatter and path for one page. The body stays on disk until opened.
#[derive(Debug, Clone)]
pub struct PageSummary {
    pub path: PathBuf,
    pub frontmatter: Frontmatter,
}

/// A page after it has been written, including any title-rewrite edits.
#[derive(Debug, Clone)]
pub struct SavedPage {
    pub path: PathBuf,
    pub frontmatter: Frontmatter,
    pub body: String,
}

/// A nation page as seen by the map editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NationInfo {
    pub id: Uuid,
    pub title: String,
    pub color: [u8; 3],
}

/// An open project folder.
#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub meta: ProjectMeta,
    pages: Vec<PageSummary>,
    maps: Vec<MapSummary>,
    pub warnings: Vec<String>,
}

impl Project {
    pub(crate) fn create(root: &Path, name: &str, start_date: Option<NaiveDate>) -> Result<Self> {
        let name = name.trim();
        if name.is_empty() {
            return Err(MotError::new("project name cannot be empty"));
        }
        fs::create_dir_all(root)?;
        let project_file = root.join("project.json");
        if project_file.exists() {
            return Err(MotError::new("folder already contains a project"));
        }
        let meta = ProjectMeta {
            schema: SCHEMA,
            name: name.to_owned(),
            start_date,
        };
        fs::write(&project_file, serde_json::to_vec_pretty(&meta)?)?;
        write_scaffold(root, name)?;
        Self::open(root)
    }

    pub(crate) fn open(root: &Path) -> Result<Self> {
        let text = fs::read_to_string(root.join("project.json"))?;
        let meta: ProjectMeta = serde_json::from_str(&text)?;
        if meta.schema != SCHEMA {
            return Err(MotError::new(format!(
                "unsupported project schema {}",
                meta.schema
            )));
        }
        let mut project = Self {
            root: root.to_path_buf(),
            meta,
            pages: Vec::new(),
            maps: Vec::new(),
            warnings: Vec::new(),
        };
        project.rescan()?;
        Ok(project)
    }

    pub(crate) fn save_meta(&self) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(&self.meta)?;
        fs::write(self.root.join("project.json"), bytes)?;
        Ok(())
    }

    pub(crate) fn rescan(&mut self) -> Result<()> {
        let mut pages = Vec::new();
        let mut warnings = Vec::new();
        walk(&self.root, &mut pages, &mut warnings)?;
        warnings.extend(duplicate_warnings(&pages));
        self.maps = scan_maps(&self.root.join(MAPS_DIR), &mut warnings);
        self.pages = pages;
        self.warnings = warnings;
        Ok(())
    }

    pub(crate) fn pages(&self) -> &[PageSummary] {
        &self.pages
    }

    /// Every nation page, sorted by title.
    pub(crate) fn nations(&self) -> Vec<NationInfo> {
        let mut nations: Vec<NationInfo> = self
            .pages
            .iter()
            .filter_map(|page| match &page.frontmatter.data {
                PageData::Nation(meta) => Some(NationInfo {
                    id: page.frontmatter.id,
                    title: page.frontmatter.title.clone(),
                    color: meta.color_for(page.frontmatter.id),
                }),
                _ => None,
            })
            .collect();
        nations.sort_by_key(|nation| nation.title.to_lowercase());
        nations
    }

    pub(crate) fn maps(&self) -> &[MapSummary] {
        &self.maps
    }

    /// Folder holding a map's files.
    pub(crate) fn map_dir(&self, slug: &str) -> Result<PathBuf> {
        if !valid_folder_name(slug) {
            return Err(MotError::new("invalid map name"));
        }
        Ok(self.root.join(MAPS_DIR).join(slug))
    }

    /// Import an image as a new map and return its slug.
    pub(crate) fn create_map(
        &mut self,
        name: &str,
        source: &Path,
        px_per_block: f64,
        origin_block: [f64; 2],
        dimension: &str,
    ) -> Result<String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(MotError::new("map name cannot be empty"));
        }
        let extension = source
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .filter(|ext| MAP_IMAGE_EXTENSIONS.contains(&ext.as_str()))
            .ok_or_else(|| MotError::new("the map image must be a PNG or JPEG file"))?;
        let (width, height) = image::image_dimensions(source)?;
        let meta = MapMeta {
            schema: MAP_SCHEMA,
            name: name.to_owned(),
            image: format!("base.{extension}"),
            image_size: [width, height],
            px_per_block,
            origin_block,
            dimension: dimension.trim().to_owned(),
        };
        meta.validate()?;

        let maps_root = self.root.join(MAPS_DIR);
        fs::create_dir_all(&maps_root)?;
        let slug = unique_dir_slug(&maps_root, &slugify(name));
        let dir = maps_root.join(&slug);
        fs::create_dir(&dir)?;
        let written = fs::copy(source, dir.join(&meta.image))
            .map_err(MotError::from)
            .and_then(|_| write_meta(&dir, &meta))
            .and_then(|()| write_claims(&dir, &ClaimLayer::default()));
        if let Err(err) = written {
            let _ = fs::remove_dir_all(&dir);
            return Err(err);
        }
        self.rescan()?;
        Ok(slug)
    }

    pub(crate) fn load_map(&self, slug: &str) -> Result<MapDoc> {
        let dir = self.map_dir(slug)?;
        Ok(MapDoc {
            slug: slug.to_owned(),
            meta: read_meta(&dir)?,
            claims: read_claims(&dir)?,
        })
    }

    pub(crate) fn save_map(&mut self, slug: &str, meta: &MapMeta, claims: &ClaimLayer) -> Result<()> {
        meta.validate()?;
        let dir = self.map_dir(slug)?;
        write_meta(&dir, meta)?;
        write_claims(&dir, claims)?;
        if let Some(summary) = self.maps.iter_mut().find(|map| map.slug == slug) {
            summary.meta = meta.clone();
            summary.counts = claims.counts();
        }
        Ok(())
    }

    pub(crate) fn delete_map(&mut self, slug: &str) -> Result<()> {
        let dir = self.map_dir(slug)?;
        fs::remove_dir_all(dir)?;
        self.rescan()?;
        Ok(())
    }

    pub(crate) fn contains(&self, id: Uuid) -> bool {
        self.summary(id).is_some()
    }

    pub(crate) fn summary(&self, id: Uuid) -> Option<&PageSummary> {
        self.pages.iter().find(|page| page.frontmatter.id == id)
    }

    pub(crate) fn resolve_title(&self, link: &str) -> Option<Uuid> {
        resolve_link(
            self.pages.iter().map(|page| {
                let stem = page.path.file_stem().and_then(|stem| stem.to_str());
                (page.frontmatter.title.as_str(), stem, page.frontmatter.id)
            }),
            link,
        )
    }

    pub(crate) fn title_taken(&self, title: &str, except: Option<Uuid>) -> bool {
        self.pages.iter().any(|page| {
            except != Some(page.frontmatter.id)
                && page.frontmatter.title.eq_ignore_ascii_case(title)
        })
    }

    pub(crate) fn relative(&self, path: &Path) -> PathBuf {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_path_buf()
    }

    pub(crate) fn load_page(&self, id: Uuid) -> Result<Page> {
        let path = self
            .summary(id)
            .ok_or_else(|| MotError::new("page is not in the project"))?
            .path
            .clone();
        Page::load(&path)
    }

    pub(crate) fn events_for_nation(&self, nation: &str) -> Vec<&PageSummary> {
        self.pages
            .iter()
            .filter(|page| event_mentions_nation(page, nation))
            .collect()
    }

    pub(crate) fn save_page(
        &mut self,
        id: Uuid,
        mut frontmatter: Frontmatter,
        body: &str,
    ) -> Result<SavedPage> {
        frontmatter.normalize();
        let (old_title, old_path) = {
            let summary = self
                .summary(id)
                .ok_or_else(|| MotError::new("page is not in the project"))?;
            (summary.frontmatter.title.clone(), summary.path.clone())
        };
        if frontmatter.title.is_empty() {
            return Err(MotError::new("title cannot be empty"));
        }
        if frontmatter.title != old_title && self.title_taken(&frontmatter.title, Some(id)) {
            return Err(MotError::new("another page already uses that title"));
        }
        let mut body = body.to_owned();
        if frontmatter.title != old_title {
            let new_title = frontmatter.title.clone();
            body = rewrite_wiki_links(&body, &old_title, &new_title);
            rewrite_title_fields(&mut frontmatter, &old_title, &new_title);
            self.rewrite_other_pages(id, &old_title, &new_title)?;
        }
        let path = self.destination_path(&old_path, &frontmatter.title);
        write_page(&path, &frontmatter, &body)?;
        if path != old_path {
            fs::remove_file(&old_path)?;
        }
        self.rescan()?;
        Ok(SavedPage {
            path,
            frontmatter,
            body,
        })
    }

    pub(crate) fn create_page(
        &mut self,
        parent: &Path,
        mut frontmatter: Frontmatter,
        body: &str,
    ) -> Result<Uuid> {
        frontmatter.normalize();
        if frontmatter.title.is_empty() {
            return Err(MotError::new("title cannot be empty"));
        }
        if self.title_taken(&frontmatter.title, None) {
            return Err(MotError::new("another page already uses that title"));
        }
        fs::create_dir_all(parent)?;
        let path = page_file_path(parent, &frontmatter.title, None);
        let id = frontmatter.id;
        write_page(&path, &frontmatter, body)?;
        self.rescan()?;
        Ok(id)
    }

    pub(crate) fn ensure_section(&mut self, name: &str) -> Result<PathBuf> {
        let dir = self.root.join(name);
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
            write_page(&dir.join("index.md"), &Frontmatter::note(section_title(name)), "")?;
            self.rescan()?;
        }
        Ok(dir)
    }

    pub(crate) fn create_folder(&mut self, parent_rel: &Path, name: &str) -> Result<Uuid> {
        let name = name.trim();
        if !valid_folder_name(name) {
            return Err(MotError::new(
                "folder name cannot be empty or contain path separators",
            ));
        }
        let dir = self.root.join(parent_rel).join(name);
        if dir.exists() {
            return Err(MotError::new("folder already exists"));
        }
        fs::create_dir(&dir)?;
        let frontmatter = Frontmatter::note(name.to_owned());
        let id = frontmatter.id;
        write_page(&dir.join("index.md"), &frontmatter, "")?;
        self.rescan()?;
        Ok(id)
    }

    pub(crate) fn delete_page(&mut self, id: Uuid) -> Result<()> {
        let path = self
            .summary(id)
            .ok_or_else(|| MotError::new("page is not in the project"))?
            .path
            .clone();
        fs::remove_file(path)?;
        self.rescan()?;
        Ok(())
    }

    pub(crate) fn delete_folder(&mut self, relative: &Path) -> Result<()> {
        if relative.as_os_str().is_empty() {
            return Err(MotError::new("cannot delete the project root"));
        }
        let absolute = self.root.join(relative);
        if absolute == self.root {
            return Err(MotError::new("cannot delete the project root"));
        }
        fs::remove_dir_all(absolute)?;
        self.rescan()?;
        Ok(())
    }

    pub(crate) fn rename_folder(&mut self, relative: &Path, name: &str) -> Result<()> {
        let name = name.trim();
        if relative.as_os_str().is_empty() {
            return Err(MotError::new("cannot rename the project root"));
        }
        if !valid_folder_name(name) {
            return Err(MotError::new(
                "folder name cannot be empty or contain path separators",
            ));
        }
        let absolute = self.root.join(relative);
        let Some(parent) = absolute.parent() else {
            return Err(MotError::new("folder has no parent"));
        };
        let destination = parent.join(name);
        if destination.exists() {
            return Err(MotError::new("a folder with that name already exists"));
        }
        fs::rename(&absolute, &destination)?;
        self.rescan()?;
        Ok(())
    }

    fn destination_path(&self, current: &Path, title: &str) -> PathBuf {
        if current
            .file_name()
            .is_some_and(|name| name == "index.md")
        {
            return current.to_path_buf();
        }
        let parent = current.parent().unwrap_or(self.root.as_path());
        page_file_path(parent, title, Some(current))
    }

    fn rewrite_other_pages(&self, except: Uuid, old: &str, new: &str) -> Result<()> {
        let others: Vec<PathBuf> = self
            .pages
            .iter()
            .filter(|page| page.frontmatter.id != except)
            .map(|page| page.path.clone())
            .collect();
        for path in others {
            rewrite_file(&path, old, new)?;
        }
        Ok(())
    }
}

fn rewrite_file(path: &Path, old: &str, new: &str) -> Result<()> {
    let page = Page::load(path)?;
    let original = page.body.contents();
    let body = rewrite_wiki_links(&original, old, new);
    let mut frontmatter = page.frontmatter.clone();
    rewrite_title_fields(&mut frontmatter, old, new);
    if frontmatter != page.frontmatter || body != original {
        write_page(path, &frontmatter, &body)?;
    }
    Ok(())
}

fn write_page(path: &Path, frontmatter: &Frontmatter, body: &str) -> Result<()> {
    Page {
        path: path.to_path_buf(),
        frontmatter: frontmatter.clone(),
        body: TextBuf::from_string(body),
    }
    .write()
}

fn write_scaffold(root: &Path, name: &str) -> Result<()> {
    write_page(&root.join("index.md"), &Frontmatter::note(name.to_owned()), "")?;
    write_page(
        &root.join("nations").join("index.md"),
        &Frontmatter::note("Nations".to_owned()),
        "",
    )?;
    write_page(
        &root.join("players").join("index.md"),
        &Frontmatter::note("Players".to_owned()),
        "",
    )?;
    write_page(
        &root.join("events").join("index.md"),
        &Frontmatter::note("Events".to_owned()),
        "",
    )?;
    Ok(())
}

fn walk(dir: &Path, pages: &mut Vec<PageSummary>, warnings: &mut Vec<String>) -> Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        if name == "project.json" || name.to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(&path, pages, warnings)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            match Page::load(&path) {
                Ok(page) => pages.push(PageSummary {
                    path,
                    frontmatter: page.frontmatter,
                }),
                Err(err) => warnings.push(format!("{}: {err}", path.display())),
            }
        }
    }
    Ok(())
}

/// List `maps/*/map.json`. Unreadable maps become warnings instead of errors.
fn scan_maps(maps_root: &Path, warnings: &mut Vec<String>) -> Vec<MapSummary> {
    let Ok(entries) = fs::read_dir(maps_root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join(crate::backend::map::MAP_FILE).is_file())
        .collect();
    dirs.sort();
    let mut maps = Vec::new();
    for dir in dirs {
        let Some(slug) = dir.file_name().and_then(|name| name.to_str()).map(str::to_owned) else {
            continue;
        };
        match read_meta(&dir).and_then(|meta| Ok((meta, read_claims(&dir)?))) {
            Ok((meta, claims)) => maps.push(MapSummary {
                slug,
                meta,
                counts: claims.counts(),
            }),
            Err(err) => warnings.push(format!("{}: {err}", dir.display())),
        }
    }
    maps
}

fn unique_dir_slug(parent: &Path, slug: &str) -> String {
    if !parent.join(slug).exists() {
        return slug.to_owned();
    }
    for suffix in 2..10_000 {
        let candidate = format!("{slug}-{suffix}");
        if !parent.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{slug}-{}", Uuid::new_v4())
}

fn duplicate_warnings(pages: &[PageSummary]) -> Vec<String> {
    let mut first = BTreeMap::new();
    let mut warnings = Vec::new();
    for page in pages {
        let key = page.frontmatter.title.to_lowercase();
        if let Some(existing) = first.get(&key) {
            warnings.push(format!(
                "Duplicate title \"{}\" (also used by {existing})",
                page.frontmatter.title
            ));
        } else {
            first.insert(key, page.frontmatter.title.clone());
        }
    }
    warnings
}

fn event_mentions_nation(page: &PageSummary, nation: &str) -> bool {
    let PageData::Event(meta) = &page.frontmatter.data else {
        return false;
    };
    meta.nations
        .iter()
        .any(|name| name.eq_ignore_ascii_case(nation))
        || meta.location.eq_ignore_ascii_case(nation)
        || meta
            .participants
            .iter()
            .any(|name| name.eq_ignore_ascii_case(nation))
}

fn section_title(name: &str) -> String {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut title = first.to_uppercase().collect::<String>();
    title.push_str(chars.as_str());
    title
}

fn valid_folder_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
}

fn page_file_path(dir: &Path, title: &str, current: Option<&Path>) -> PathBuf {
    let mut slug = slugify(title);
    if slug == "index" {
        slug = "index-page".to_owned();
    }
    unique_file(dir, &slug, current)
}

fn unique_file(dir: &Path, slug: &str, current: Option<&Path>) -> PathBuf {
    let direct = dir.join(format!("{slug}.md"));
    if current.is_some_and(|path| path == direct) || !direct.exists() {
        return direct;
    }
    for suffix in 2..10_000 {
        let candidate = dir.join(format!("{slug}-{suffix}.md"));
        if current.is_some_and(|path| path == candidate) || !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{slug}-{}.md", Uuid::new_v4()))
}

fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut hyphen = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            hyphen = false;
        } else if !hyphen && !slug.is_empty() {
            slug.push('-');
            hyphen = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "page".to_owned()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::Project;
    use crate::backend::page::{PageData, PageKind};
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("mot-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).expect("temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn map_create_save_reload_and_delete() {
        use crate::backend::map::{ChunkPos, ClaimLayer};

        let dir = TempDir::new();
        let mut project = Project::create(&dir.0, "Campaign", None).expect("create");
        let source = dir.0.join("export.png");
        image::RgbaImage::new(64, 48).save(&source).expect("write png");

        let slug = project
            .create_map("Overworld", &source, 1.0, [-32.0, -24.0], "Overworld")
            .expect("create map");
        assert_eq!(slug, "overworld");
        assert!(project.map_dir(&slug).expect("dir").join("base.png").is_file());
        assert_eq!(project.maps().len(), 1);
        assert_eq!(project.maps()[0].meta.image_size, [64, 48]);
        assert!(
            project
                .create_map("Bad", &dir.0.join("notes.txt"), 1.0, [0.0, 0.0], "")
                .is_err(),
            "non-image extensions are rejected"
        );
        assert!(
            project.create_map("Zero", &source, 0.0, [0.0, 0.0], "").is_err(),
            "scale must be positive"
        );
        assert_eq!(project.maps().len(), 1, "failed imports leave nothing behind");

        let again = project
            .create_map("Overworld", &source, 2.0, [0.0, 0.0], "")
            .expect("second map");
        assert_eq!(again, "overworld-2");

        let nation = project
            .create_page(
                &dir.0.join("nations"),
                crate::backend::page::Frontmatter::nation("Rome".to_owned()),
                "",
            )
            .expect("nation");
        let mut doc = project.load_map(&slug).expect("load");
        assert!(doc.claims.is_empty());
        doc.claims.set(ChunkPos::new(-1, -1), Some(nation));
        doc.claims.set(ChunkPos::new(0, -1), Some(nation));
        doc.meta.px_per_block = 4.0;
        project.save_map(&slug, &doc.meta, &doc.claims).expect("save");
        assert_eq!(project.maps()[0].counts.get(&nation), Some(&2));

        let reopened = Project::open(&dir.0).expect("reopen");
        let reloaded = reopened.load_map(&slug).expect("reload");
        assert_eq!(reloaded.meta.px_per_block, 4.0);
        let expected: ClaimLayer = doc.claims;
        assert_eq!(reloaded.claims, expected);
        assert_eq!(reopened.nations().len(), 1);
        assert_eq!(reopened.nations()[0].id, nation);

        project.delete_map(&slug).expect("delete");
        assert_eq!(project.maps().len(), 1);
        assert!(project.maps().iter().all(|map| map.slug != slug));
    }

    #[test]
    fn rename_rewrites_links_and_nation_field() {
        let dir = TempDir::new();
        let mut project = Project::create(&dir.0, "Campaign", None).expect("create");
        let rome = project
            .create_page(
                &dir.0.join("nations"),
                crate::backend::page::Frontmatter::nation("Rome".to_owned()),
                "Ruled by [[Caesar]].\n",
            )
            .expect("rome");
        let mut caesar = crate::backend::page::Frontmatter::player("Caesar".to_owned());
        caesar.player_mut().expect("player").nation = Some("Rome".to_owned());
        let player_id = project
            .create_page(&dir.0.join("players"), caesar, "Citizen of [[Rome]].\n")
            .expect("player");

        let loaded = project.load_page(player_id).expect("load");
        let mut renamed = loaded.frontmatter.clone();
        renamed.title = "Julius".to_owned();
        project
            .save_page(player_id, renamed, &loaded.body.contents())
            .expect("rename player");

        let rome_page = project.load_page(rome).expect("reload rome");
        assert!(rome_page.body.contents().contains("[[Julius]]"));
        assert!(!rome_page.body.contents().contains("[[Caesar]]"));

        let mut rome_fm = rome_page.frontmatter.clone();
        rome_fm.title = "Italia".to_owned();
        project
            .save_page(rome, rome_fm, &rome_page.body.contents())
            .expect("rename nation");
        let player = project.load_page(player_id).expect("reload player");
        assert!(player.body.contents().contains("[[Italia]]"));
        match player.frontmatter.data {
            PageData::Player(ref meta) => assert_eq!(meta.nation.as_deref(), Some("Italia")),
            other => panic!("expected player, got {other:?}"),
        }
        assert_eq!(player.frontmatter.kind(), PageKind::Player);
    }
}
