#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderError {
    pub kind: LoaderErrorKind,
    pub source: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderErrorKind {
    EmptyManifest,
    EnvironmentVariable,
    ManifestParsing,
    ResourceValidation,
    ReferenceResolution,
    ScriptValidation,
}

impl LoaderError {
    pub(crate) fn new(kind: LoaderErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            source: None,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for LoaderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(source) = &self.source {
            return write!(formatter, "{source}: {}", self.message);
        }
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LoaderError {}
