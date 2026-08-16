use std::fmt;

macro_rules! string_id {
    ($name:ident, $label:literal) => {
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                let value = value.into();
                if value.is_empty() {
                    return Err(IdError::Empty { kind: $label });
                }
                if value.trim() != value {
                    return Err(IdError::Untrimmed {
                        kind: $label,
                        value,
                    });
                }
                if value.chars().any(char::is_control) {
                    return Err(IdError::ControlCharacter {
                        kind: $label,
                        value,
                    });
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&self.0)
                    .finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

string_id!(CapabilityId, "capability");
string_id!(CapabilityOwnerId, "capability owner");
string_id!(ScopeId, "scope");

impl ScopeId {
    pub fn process() -> Self {
        Self("process".to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdError {
    Empty { kind: &'static str },
    Untrimmed { kind: &'static str, value: String },
    ControlCharacter { kind: &'static str, value: String },
}

impl fmt::Display for IdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { kind } => write!(formatter, "{kind} id cannot be empty"),
            Self::Untrimmed { kind, value } => {
                write!(formatter, "{kind} id must be trimmed: {value:?}")
            }
            Self::ControlCharacter { kind, value } => {
                write!(
                    formatter,
                    "{kind} id contains a control character: {value:?}"
                )
            }
        }
    }
}

impl std::error::Error for IdError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenerationId(u64);

impl GenerationId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for GenerationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
