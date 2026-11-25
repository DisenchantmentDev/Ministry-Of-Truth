/// Structured fields stored on a player page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerMeta {
    pub nation: Option<String>,
    pub discord: Option<String>,
    pub death: Option<i32>,
}
