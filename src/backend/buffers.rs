use ropey::Rope;

/// Rope-backed document body.
///
/// The editor works on a `String` because `egui::TextEdit` cannot edit a rope.
/// Save paths copy that string back into the rope before writing the file.
#[derive(Debug, Clone)]
pub struct TextBuf {
    rope: Rope,
}

impl Default for TextBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl TextBuf {
    pub fn new() -> Self {
        Self { rope: Rope::new() }
    }

    pub fn from_string(content: &str) -> Self {
        Self {
            rope: Rope::from_str(content),
        }
    }

    /// Full document text.
    pub fn contents(&self) -> String {
        self.rope.to_string()
    }

    #[allow(dead_code)]
    pub fn insert(&mut self, pos: usize, text: &str) {
        self.rope.insert(pos, text);
    }

    #[allow(dead_code)]
    pub fn remove(&mut self, pos: usize, len: usize) {
        self.rope.remove(pos..pos + len);
    }
}

impl From<&str> for TextBuf {
    fn from(value: &str) -> Self {
        Self::from_string(value)
    }
}

impl From<String> for TextBuf {
    fn from(value: String) -> Self {
        Self::from_string(&value)
    }
}

impl std::fmt::Display for TextBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.rope)
    }
}
