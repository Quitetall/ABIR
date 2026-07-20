use alloc::string::{String, ToString};
use core::fmt;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConceptId(String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConceptError;

impl ConceptId {
    pub fn new(value: impl AsRef<str>) -> Result<Self, ConceptError> {
        let value = value.as_ref();
        let (namespace, local) = value.split_once(':').ok_or(ConceptError)?;
        let mut chars = namespace.chars();
        if !matches!(chars.next(), Some('a'..='z'))
            || !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
            || local.is_empty()
            || !local
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
        {
            return Err(ConceptError);
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConceptId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for ConceptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("concept identifier is not canonical")
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SourceKey {
    namespace: String,
    value: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceKeyError;

impl SourceKey {
    pub fn new(namespace: impl AsRef<str>, value: impl AsRef<str>) -> Result<Self, SourceKeyError> {
        let namespace = namespace.as_ref();
        let value = value.as_ref();
        if namespace.is_empty()
            || namespace.chars().any(char::is_control)
            || value.chars().any(char::is_control)
        {
            return Err(SourceKeyError);
        }
        Ok(Self {
            namespace: namespace.to_string(),
            value: value.to_string(),
        })
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for SourceKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("source key namespace or value is invalid")
    }
}
