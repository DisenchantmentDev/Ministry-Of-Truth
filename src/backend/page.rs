use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::backend::buffers::TextBuf;
use crate::backend::error::{MotError, Result};
use crate::backend::event::{EventMeta, EventType};
use crate::backend::nation::{NationMeta, Relation};
use crate::backend::player::PlayerMeta;

/// Which structured record a markdown file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageKind {
    Note,
    Nation,
    Player,
    Event,
}

impl PageKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Nation => "Nation",
            Self::Player => "Player",
            Self::Event => "Event",
        }
    }
}

/// Kind-specific frontmatter payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageData {
    Note,
    Nation(NationMeta),
    Player(PlayerMeta),
    Event(EventMeta),
}

impl PageData {
    pub(crate) fn kind(&self) -> PageKind {
        match self {
            Self::Note => PageKind::Note,
            Self::Nation(_) => PageKind::Nation,
            Self::Player(_) => PageKind::Player,
            Self::Event(_) => PageKind::Event,
        }
    }
}

/// Identity, title, and structured fields for one markdown file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmatter {
    pub id: Uuid,
    pub title: String,
    pub data: PageData,
}

impl Frontmatter {
    pub(crate) fn note(title: String) -> Self {
        Self::new(title, PageData::Note)
    }

    pub(crate) fn nation(title: String) -> Self {
        Self::new(title, PageData::Nation(NationMeta::default()))
    }

    pub(crate) fn player(title: String) -> Self {
        Self::new(title, PageData::Player(PlayerMeta::default()))
    }

    pub(crate) fn event(title: String, event_type: EventType) -> Self {
        Self::new(title, PageData::Event(EventMeta::new(event_type)))
    }

    fn new(title: String, data: PageData) -> Self {
        Self {
            id: Uuid::new_v4(),
            title,
            data,
        }
    }

    pub(crate) fn kind(&self) -> PageKind {
        self.data.kind()
    }

    pub(crate) fn nation_mut(&mut self) -> Option<&mut NationMeta> {
        match &mut self.data {
            PageData::Nation(meta) => Some(meta),
            _ => None,
        }
    }

    pub(crate) fn player_mut(&mut self) -> Option<&mut PlayerMeta> {
        match &mut self.data {
            PageData::Player(meta) => Some(meta),
            _ => None,
        }
    }

    pub(crate) fn event_mut(&mut self) -> Option<&mut EventMeta> {
        match &mut self.data {
            PageData::Event(meta) => Some(meta),
            _ => None,
        }
    }

    /// Trim text fields and drop blank list rows before writing.
    pub(crate) fn normalize(&mut self) {
        self.title = self.title.trim().to_owned();
        match &mut self.data {
            PageData::Note => {}
            PageData::Nation(meta) => {
                meta.location = meta.location.trim().to_owned();
                trim_list(&mut meta.leaders);
                meta.relations
                    .retain(|relation| !relation.target.trim().is_empty());
                for relation in &mut meta.relations {
                    relation.target = relation.target.trim().to_owned();
                }
            }
            PageData::Player(meta) => {
                meta.nation = trimmed_option(meta.nation.take());
                meta.discord = trimmed_option(meta.discord.take());
            }
            PageData::Event(meta) => {
                meta.location = meta.location.trim().to_owned();
                trim_list(&mut meta.nations);
                trim_list(&mut meta.participants);
            }
        }
    }
}

fn trim_list(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    for value in values {
        *value = value.trim().to_owned();
    }
}

fn trimmed_option(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// A markdown page on disk. The body lives in a rope and is loaded on demand.
#[derive(Debug, Clone)]
pub struct Page {
    pub path: PathBuf,
    pub frontmatter: Frontmatter,
    pub body: TextBuf,
}

impl Page {
    pub(crate) fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)?;
        let (frontmatter, body) = parse_document(&text)?;
        Ok(Self {
            path: path.to_path_buf(),
            frontmatter,
            body: TextBuf::from_string(&body),
        })
    }

    pub(crate) fn write(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = write_document(&self.frontmatter, &self.body.contents())?;
        fs::write(&self.path, text)?;
        Ok(())
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct StoredFrontmatter {
    id: Uuid,
    kind: PageKind,
    title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    leaders: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    location: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    founded: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    relations: Vec<Relation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    nation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    discord: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    death: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    day: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event_type: Option<EventType>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    nations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    participants: Vec<String>,
}

impl StoredFrontmatter {
    fn from_frontmatter(frontmatter: &Frontmatter) -> Self {
        let mut stored = Self {
            id: frontmatter.id,
            kind: frontmatter.kind(),
            title: frontmatter.title.clone(),
            leaders: Vec::new(),
            location: String::new(),
            founded: None,
            relations: Vec::new(),
            nation: None,
            discord: None,
            death: None,
            day: None,
            event_type: None,
            nations: Vec::new(),
            participants: Vec::new(),
        };
        match &frontmatter.data {
            PageData::Note => {}
            PageData::Nation(meta) => {
                stored.leaders.clone_from(&meta.leaders);
                stored.location.clone_from(&meta.location);
                stored.founded = meta.founded;
                stored.relations.clone_from(&meta.relations);
            }
            PageData::Player(meta) => {
                stored.nation.clone_from(&meta.nation);
                stored.discord.clone_from(&meta.discord);
                stored.death = meta.death;
            }
            PageData::Event(meta) => {
                stored.day = meta.day;
                stored.event_type = Some(meta.event_type);
                stored.nations.clone_from(&meta.nations);
                stored.participants.clone_from(&meta.participants);
                stored.location.clone_from(&meta.location);
            }
        }
        stored
    }

    fn into_frontmatter(self) -> Frontmatter {
        let data = match self.kind {
            PageKind::Note => PageData::Note,
            PageKind::Nation => PageData::Nation(NationMeta {
                leaders: self.leaders,
                location: self.location,
                founded: self.founded,
                relations: self.relations,
            }),
            PageKind::Player => PageData::Player(PlayerMeta {
                nation: self.nation,
                discord: self.discord,
                death: self.death,
            }),
            PageKind::Event => PageData::Event(EventMeta {
                day: self.day,
                event_type: self.event_type.unwrap_or_default(),
                nations: self.nations,
                participants: self.participants,
                location: self.location,
            }),
        };
        Frontmatter {
            id: self.id,
            title: self.title,
            data,
        }
    }
}

pub(crate) fn parse_document(text: &str) -> Result<(Frontmatter, String)> {
    let (yaml, body) = split_frontmatter(text)?;
    let stored: StoredFrontmatter = serde_yml::from_str(yaml)?;
    Ok((stored.into_frontmatter(), body.to_owned()))
}

pub(crate) fn write_document(frontmatter: &Frontmatter, body: &str) -> Result<String> {
    let yaml = serde_yml::to_string(&StoredFrontmatter::from_frontmatter(frontmatter))?;
    let mut out = String::from("---\n");
    out.push_str(&yaml);
    if !yaml.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("---\n");
    out.push_str(body);
    Ok(out)
}

fn split_frontmatter(text: &str) -> Result<(&str, &str)> {
    let rest = if let Some(rest) = text.strip_prefix("---\r\n") {
        rest
    } else if let Some(rest) = text.strip_prefix("---\n") {
        rest
    } else {
        return Err(MotError::new("page is missing frontmatter"));
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" {
            let yaml = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return Ok((yaml, body));
        }
        offset += line.len();
    }
    Err(MotError::new("page frontmatter is not closed"))
}

#[cfg(test)]
mod tests {
    use super::{parse_document, write_document, Frontmatter};
    use crate::backend::event::EventType;
    use crate::backend::nation::{Relation, Stance};

    #[test]
    fn frontmatter_round_trip() {
        let mut nation = Frontmatter::nation("Rome".to_owned());
        {
            let meta = nation.nation_mut().expect("nation");
            meta.leaders = vec!["Caesar".to_owned()];
            meta.location = "Italy".to_owned();
            meta.founded = Some(-10);
            meta.relations = vec![Relation {
                target: "Gaul".to_owned(),
                stance: Stance::Hostile,
            }];
        }
        assert_round_trip(&nation, "The republic.\n");

        let mut player = Frontmatter::player("Caesar".to_owned());
        {
            let meta = player.player_mut().expect("player");
            meta.nation = Some("Rome".to_owned());
            meta.discord = Some("caesar".to_owned());
            meta.death = Some(4);
        }
        assert_round_trip(&player, "Notes.\n");

        let mut event = Frontmatter::event("Ides".to_owned(), EventType::Assassination);
        {
            let meta = event.event_mut().expect("event");
            meta.day = Some(3);
            meta.nations = vec!["Rome".to_owned()];
            meta.participants = vec!["Brutus".to_owned(), "Caesar".to_owned()];
            meta.location = "Senate".to_owned();
        }
        assert_round_trip(&event, "## Target\n");
    }

    fn assert_round_trip(frontmatter: &Frontmatter, body: &str) {
        let text = write_document(frontmatter, body).expect("write");
        let (parsed, parsed_body) = parse_document(&text).expect("parse");
        assert_eq!(&parsed, frontmatter);
        assert_eq!(parsed_body, body);
    }
}
