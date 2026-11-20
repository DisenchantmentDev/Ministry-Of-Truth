/// Kind of story event recorded on an event page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventType {
    Assassination,
    War,
    Battle,
    Diplomacy,
    Other,
}

impl EventType {
    pub(crate) const ALL: [Self; 5] = [
        Self::Assassination,
        Self::War,
        Self::Battle,
        Self::Diplomacy,
        Self::Other,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Assassination => "Assassination",
            Self::War => "War",
            Self::Battle => "Battle",
            Self::Diplomacy => "Diplomacy",
            Self::Other => "Other",
        }
    }
}

impl Default for EventType {
    fn default() -> Self {
        Self::Other
    }
}

/// Structured fields stored on an event page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventMeta {
    pub day: Option<i32>,
    pub event_type: EventType,
    pub nations: Vec<String>,
    pub participants: Vec<String>,
    pub location: String,
}

impl EventMeta {
    pub(crate) fn new(event_type: EventType) -> Self {
        Self {
            day: Some(0),
            event_type,
            nations: Vec::new(),
            participants: Vec::new(),
            location: String::new(),
        }
    }
}
