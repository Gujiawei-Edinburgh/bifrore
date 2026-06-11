pub type ExtensionResult<T> = Result<T, ExtensionError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionError {
    pub kind: ExtensionErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionErrorKind {
    InvalidArgument,
    State,
    Emit,
    Script,
}

impl ExtensionError {
    pub fn new(kind: ExtensionErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new(ExtensionErrorKind::InvalidArgument, message)
    }

    pub fn state(message: impl Into<String>) -> Self {
        Self::new(ExtensionErrorKind::State, message)
    }

    pub fn emit(message: impl Into<String>) -> Self {
        Self::new(ExtensionErrorKind::Emit, message)
    }

    pub fn script(message: impl Into<String>) -> Self {
        Self::new(ExtensionErrorKind::Script, message)
    }
}

impl std::fmt::Display for ExtensionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for ExtensionError {}
