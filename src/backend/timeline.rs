use uuid::Uuid;

use crate::backend::page::{Frontmatter, PageData, PageKind};

/// Why a page appears on the project timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineKind {
    Founding,
    Event,
    Death,
}

impl TimelineKind {
    fn order(self) -> u8 {
        match self {
            Self::Founding => 0,
            Self::Event => 1,
            Self::Death => 2,
        }
    }
}

/// One row on the project timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEntry {
    pub day: Option<i32>,
    pub kind: TimelineKind,
    pub page_id: Uuid,
    pub title: String,
    pub detail: String,
}

/// Events, nation foundings on Day 0 or later, and recorded player deaths.
///
/// A nation founded before Day 0 is omitted. Events with no day are kept and
/// sort after every dated row.
pub(crate) fn collect_timeline<'a>(
    pages: impl IntoIterator<Item = &'a Frontmatter>,
) -> Vec<TimelineEntry> {
    let mut entries = Vec::new();
    for page in pages {
        match &page.data {
            PageData::Nation(meta) => {
                if let Some(day) = meta.founded.filter(|day| *day >= 0) {
                    entries.push(entry(
                        Some(day),
                        TimelineKind::Founding,
                        page,
                        "Founded".to_owned(),
                    ));
                }
            }
            PageData::Player(meta) => {
                if let Some(day) = meta.death {
                    entries.push(entry(
                        Some(day),
                        TimelineKind::Death,
                        page,
                        "Died".to_owned(),
                    ));
                }
            }
            PageData::Event(meta) => {
                let detail = meta.event_type.label().to_owned();
                entries.push(entry(meta.day, TimelineKind::Event, page, detail));
            }
            PageData::Note => {}
        }
    }
    entries.sort_by(|left, right| {
        left.day
            .is_none()
            .cmp(&right.day.is_none())
            .then(left.day.unwrap_or(0).cmp(&right.day.unwrap_or(0)))
            .then(left.kind.order().cmp(&right.kind.order()))
            .then(left.title.to_lowercase().cmp(&right.title.to_lowercase()))
    });
    entries
}

fn entry(day: Option<i32>, kind: TimelineKind, page: &Frontmatter, detail: String) -> TimelineEntry {
    debug_assert_ne!(page.kind(), PageKind::Note);
    TimelineEntry {
        day,
        kind,
        page_id: page.id,
        title: page.title.clone(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::collect_timeline;
    use crate::backend::event::EventType;
    use crate::backend::page::Frontmatter;

    #[test]
    fn timeline_omits_pre_day_zero_foundings_and_keeps_events_and_deaths() {
        let mut early = Frontmatter::nation("Early".to_owned());
        early.nation_mut().expect("nation").founded = Some(-10);

        let mut rome = Frontmatter::nation("Rome".to_owned());
        rome.nation_mut().expect("nation").founded = Some(0);

        let mut battle = Frontmatter::event("Skirmish".to_owned(), EventType::Battle);
        battle.event_mut().expect("event").day = Some(1);

        let mut undated = Frontmatter::event("Rumor".to_owned(), EventType::Other);
        undated.event_mut().expect("event").day = None;

        let mut caesar = Frontmatter::player("Caesar".to_owned());
        caesar.player_mut().expect("player").death = Some(1);

        let brutus = Frontmatter::player("Brutus".to_owned());

        let entries = collect_timeline([&early, &rome, &battle, &undated, &caesar, &brutus]);
        let titles: Vec<&str> = entries.iter().map(|entry| entry.title.as_str()).collect();
        assert_eq!(titles, vec!["Rome", "Skirmish", "Caesar", "Rumor"]);
        assert!(entries.iter().all(|entry| entry.title != "Early"));
        assert!(entries.iter().all(|entry| entry.title != "Brutus"));
    }
}
