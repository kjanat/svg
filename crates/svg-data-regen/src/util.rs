//! Small shared helpers for the regeneration pipeline.

/// Wrap a message as a boxed error.
pub fn boxed(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::<dyn std::error::Error>::from(message.into())
}

/// Collapse whitespace runs into single ASCII spaces.
pub fn normalize_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether a value grammar token is a bare keyword.
pub fn is_keyword_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}
