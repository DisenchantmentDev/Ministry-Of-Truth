/// How one nation regards another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stance {
    Allied,
    Hostile,
    Neutral,
    Vassal,
}

impl Stance {
    pub(crate) const ALL: [Self; 4] = [Self::Allied, Self::Hostile, Self::Neutral, Self::Vassal];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Allied => "Allied",
            Self::Hostile => "Hostile",
            Self::Neutral => "Neutral",
            Self::Vassal => "Vassal",
        }
    }
}

impl Default for Stance {
    fn default() -> Self {
        Self::Neutral
    }
}

/// A directed relation from the owning nation to `target`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Relation {
    pub target: String,
    pub stance: Stance,
}

/// Structured fields stored on a nation page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NationMeta {
    pub leaders: Vec<String>,
    pub location: String,
    pub founded: Option<i32>,
    pub relations: Vec<Relation>,
}

impl Default for NationMeta {
    fn default() -> Self {
        Self {
            leaders: Vec::new(),
            location: String::new(),
            founded: None,
            relations: Vec::new(),
        }
    }
}
