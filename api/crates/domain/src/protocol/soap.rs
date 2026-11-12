//! SOAP 1.1 / 1.2 over HTTP POST.

use super::default_header;
use crate::body::{RawLanguage, RequestBody};
use crate::request::{HttpMethod, HttpRequest, KeyValue};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoapVersion {
    #[default]
    V11,
    V12,
}

impl SoapVersion {
    pub const ALL: [Self; 2] = [Self::V11, Self::V12];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::V11 => "SOAP 1.1",
            Self::V12 => "SOAP 1.2",
        }
    }

    /// Envelope namespace.
    #[must_use]
    pub const fn namespace(self) -> &'static str {
        match self {
            Self::V11 => "http://schemas.xmlsoap.org/soap/envelope/",
            Self::V12 => "http://www.w3.org/2003/05/soap-envelope",
        }
    }

    /// `Content-Type` without the action parameter.
    #[must_use]
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::V11 => "text/xml; charset=utf-8",
            Self::V12 => "application/soap+xml; charset=utf-8",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SoapSpec {
    pub version: SoapVersion,
    /// SOAPAction (1.1 header) / `action` content-type parameter (1.2).
    pub action: String,
    /// The whole envelope.
    pub envelope: String,
    /// Where the WSDL was loaded from (to reload it).
    pub wsdl_url: String,
    /// Operation picked from the WSDL (informational).
    pub operation: String,
}

/// The headers a SOAP request adds unless the user set them.
#[must_use]
pub fn soap_headers(spec: &SoapSpec) -> Vec<KeyValue> {
    let action = spec.action.trim();
    match spec.version {
        SoapVersion::V11 => vec![
            KeyValue::new("Content-Type", SoapVersion::V11.content_type()),
            KeyValue::new("SOAPAction", format!("\"{action}\"")),
        ],
        SoapVersion::V12 if action.is_empty() => {
            vec![KeyValue::new(
                "Content-Type",
                SoapVersion::V12.content_type(),
            )]
        }
        SoapVersion::V12 => vec![KeyValue::new(
            "Content-Type",
            format!("{}; action=\"{action}\"", SoapVersion::V12.content_type()),
        )],
    }
}

pub(super) fn lower(request: &HttpRequest) -> HttpRequest {
    let spec = &request.specs.soap;
    let mut out = request.clone();
    out.method = HttpMethod::Post;
    for h in soap_headers(spec) {
        default_header(&mut out.headers, &h.key, h.value);
    }
    out.body = RequestBody::Raw {
        language: RawLanguage::Xml,
        text: spec.envelope.clone(),
    };
    out
}

/// An envelope around `body` (already indented XML, may be empty).
#[must_use]
pub fn envelope(version: SoapVersion, body: &str) -> String {
    let ns = version.namespace();
    let inner = if body.trim().is_empty() {
        "    <!-- request element -->".to_owned()
    } else {
        body.lines()
            .map(|l| format!("    {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<soap:Envelope xmlns:soap=\"{ns}\">\n  <soap:Header/>\n  <soap:Body>\n{inner}\n  </soap:Body>\n</soap:Envelope>"
    )
}
