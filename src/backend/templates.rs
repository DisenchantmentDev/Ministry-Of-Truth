use crate::backend::event::EventType;
use crate::backend::page::Frontmatter;
use crate::backend::project::{EVENTS_DIR, NATIONS_DIR, PLAYERS_DIR};

/// A scaffold copied into a new page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKind {
    Note,
    Assassination,
    War,
    Player,
    Nation,
    Event,
}

impl TemplateKind {
    pub(crate) const MENU: [Self; 5] = [
        Self::Assassination,
        Self::War,
        Self::Player,
        Self::Nation,
        Self::Note,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Note => "Blank note",
            Self::Assassination => "Assassination",
            Self::War => "War",
            Self::Player => "Player",
            Self::Nation => "Nation / group",
            Self::Event => "Event",
        }
    }

    /// Folder name under the project root, when the template has a fixed home.
    pub(crate) fn section(self) -> Option<&'static str> {
        match self {
            Self::Note => None,
            Self::Assassination | Self::War | Self::Event => Some(EVENTS_DIR),
            Self::Player => Some(PLAYERS_DIR),
            Self::Nation => Some(NATIONS_DIR),
        }
    }
}

pub(crate) fn instantiate(template: TemplateKind, title: String) -> (Frontmatter, String) {
    let frontmatter = match template {
        TemplateKind::Note => Frontmatter::note(title),
        TemplateKind::Assassination => Frontmatter::event(title, EventType::Assassination),
        TemplateKind::War => Frontmatter::event(title, EventType::War),
        TemplateKind::Player => Frontmatter::player(title),
        TemplateKind::Nation => Frontmatter::nation(title),
        TemplateKind::Event => Frontmatter::event(title, EventType::Other),
    };
    let body = match template {
        TemplateKind::Note => include_str!("templates/note.md"),
        TemplateKind::Assassination => include_str!("templates/assassination.md"),
        TemplateKind::War => include_str!("templates/war.md"),
        TemplateKind::Player => include_str!("templates/player.md"),
        TemplateKind::Nation => include_str!("templates/nation.md"),
        TemplateKind::Event => "",
    };
    (frontmatter, body.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{instantiate, TemplateKind};
    use crate::backend::event::EventType;
    use crate::backend::page::{PageData, PageKind};

    #[test]
    fn assassination_template_is_an_assassination_event() {
        let (frontmatter, body) = instantiate(TemplateKind::Assassination, "Ides".to_owned());
        assert_eq!(frontmatter.kind(), PageKind::Event);
        match frontmatter.data {
            PageData::Event(meta) => assert_eq!(meta.event_type, EventType::Assassination),
            other => panic!("expected an event page, got {other:?}"),
        }
        assert!(body.contains("## Target"));
        assert!(body.contains("## Perpetrator"));
    }
}
