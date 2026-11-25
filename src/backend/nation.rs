use uuid::Uuid;

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
    /// Map tint chosen by the user. `None` falls back to [`derived_color`].
    pub color: Option<[u8; 3]>,
}

impl NationMeta {
    /// The color used for this nation on maps.
    pub(crate) fn color_for(&self, id: Uuid) -> [u8; 3] {
        self.color.unwrap_or_else(|| derived_color(id))
    }
}

impl Default for NationMeta {
    fn default() -> Self {
        Self {
            leaders: Vec::new(),
            location: String::new(),
            founded: None,
            relations: Vec::new(),
            color: None,
        }
    }
}

/// A stable, well-spread color for a nation page that has no explicit color.
///
/// Hues step by the golden angle from a number taken from the page id, so ids
/// that differ produce visibly different colors.
pub(crate) fn derived_color(id: Uuid) -> [u8; 3] {
    let bytes = id.as_bytes();
    let seed = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let hue = (f64::from(seed) * GOLDEN_ANGLE_DEGREES) % 360.0;
    hsl_to_rgb(hue, 0.75, 0.5)
}

const GOLDEN_ANGLE_DEGREES: f64 = 137.507_764_05;

fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> [u8; 3] {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue / 60.0;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (red, green, blue) = match sector {
        s if s < 1.0 => (chroma, second, 0.0),
        s if s < 2.0 => (second, chroma, 0.0),
        s if s < 3.0 => (0.0, chroma, second),
        s if s < 4.0 => (0.0, second, chroma),
        s if s < 5.0 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let offset = lightness - chroma / 2.0;
    [red, green, blue].map(|channel| to_byte(channel + offset))
}

fn to_byte(value: f64) -> u8 {
    // Clamped to 0..=255 first, so the cast cannot truncate.
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Parse `#rrggbb` (the `#` is optional).
pub(crate) fn parse_hex_color(text: &str) -> Option<[u8; 3]> {
    let digits = text.trim().trim_start_matches('#');
    if digits.len() != 6 || !digits.is_ascii() {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&digits[range], 16).ok();
    Some([channel(0..2)?, channel(2..4)?, channel(4..6)?])
}

/// Format a color as `#rrggbb`.
pub(crate) fn format_hex_color(color: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

#[cfg(test)]
mod tests {
    use super::{derived_color, format_hex_color, parse_hex_color};
    use uuid::Uuid;

    #[test]
    fn derived_color_is_stable_and_distinct() {
        let first = Uuid::from_u128(0x0000_0001_0000_0000_0000_0000_0000_0000);
        let second = Uuid::from_u128(0x0000_0002_0000_0000_0000_0000_0000_0000);
        assert_eq!(derived_color(first), derived_color(first));
        assert_ne!(derived_color(first), derived_color(second));
    }

    #[test]
    fn hex_round_trip() {
        assert_eq!(parse_hex_color("#ff8000"), Some([255, 128, 0]));
        assert_eq!(parse_hex_color("00ff10"), Some([0, 255, 16]));
        assert_eq!(parse_hex_color("#12345"), None);
        assert_eq!(parse_hex_color("#gg0000"), None);
        assert_eq!(format_hex_color([255, 128, 0]), "#ff8000");
    }
}
