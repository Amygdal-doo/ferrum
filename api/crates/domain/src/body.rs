//! Request bodies.

use crate::request::KeyValue;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawLanguage {
    #[default]
    Json,
    Text,
    Xml,
    Html,
}

impl RawLanguage {
    pub const ALL: [Self; 4] = [Self::Json, Self::Text, Self::Xml, Self::Html];

    /// Default `Content-Type`, applied unless the user set one explicitly.
    #[must_use]
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::Text => "text/plain",
            Self::Xml => "application/xml",
            Self::Html => "text/html",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Text => "Text",
            Self::Xml => "XML",
            Self::Html => "HTML",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormFieldKind {
    #[default]
    Text,
    /// `value` is an absolute file path.
    File,
}

/// One multipart form-data row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormField {
    pub key: String,
    pub value: String,
    pub kind: FormFieldKind,
    pub enabled: bool,
}

impl Default for FormField {
    fn default() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            kind: FormFieldKind::Text,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RequestBody {
    #[default]
    None,
    Raw {
        language: RawLanguage,
        text: String,
    },
    UrlEncoded {
        fields: Vec<KeyValue>,
    },
    FormData {
        fields: Vec<FormField>,
    },
    Binary {
        /// Absolute file path; `None` until a file is chosen.
        path: Option<String>,
    },
}

impl RequestBody {
    #[must_use]
    pub const fn mode_label(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Raw { .. } => "raw",
            Self::UrlEncoded { .. } => "x-www-form-urlencoded",
            Self::FormData { .. } => "form-data",
            Self::Binary { .. } => "binary",
        }
    }
}
