#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitKey {
    Enter,
    NumpadEnter,
    Other,
}

pub struct InputPolicy;

impl InputPolicy {
    pub fn should_submit(key: SubmitKey, shift: bool, marked: bool) -> bool {
        matches!(key, SubmitKey::Enter | SubmitKey::NumpadEnter) && !shift && !marked
    }
}
