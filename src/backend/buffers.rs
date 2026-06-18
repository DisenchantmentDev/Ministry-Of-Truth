use ropey::Rope;

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

    pub fn insert(&mut self, pos: usize, text: &str) {
        self.rope.insert(pos, text);
    }

    pub fn remove(&mut self, pos: usize, len: usize) {
        self.rope.remove(pos..pos + len);
    }
}

impl From<&str> for TextBuf {
    fn from(s: &str) -> Self {
        Self::from_string(s)
    }
}

impl From<String> for TextBuf {
    fn from(s: String) -> Self {
        Self::from_string(&s)
    }
}

impl std::fmt::Display for TextBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.rope)
    }
}
