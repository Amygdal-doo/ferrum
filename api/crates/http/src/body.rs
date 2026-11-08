//! Turns a [`RequestBody`] into bytes or a multipart form. Files are read
//! asynchronously; read failures become `ExecError::File("path: reason")`.

use ferrumapi_domain::{ExecError, FormFieldKind, RequestBody};
use reqwest::multipart::{Form, Part};
use std::path::Path;

pub(crate) const URLENCODED: &str = "application/x-www-form-urlencoded";
pub(crate) const OCTET_STREAM: &str = "application/octet-stream";

pub(crate) enum PreparedBody {
    Empty,
    /// Bytes plus the default `Content-Type` used when the user set none.
    Bytes {
        data: Vec<u8>,
        content_type: &'static str,
    },
    Multipart(Form),
}

pub(crate) async fn prepare_body(body: &RequestBody) -> Result<PreparedBody, ExecError> {
    Ok(match body {
        RequestBody::None => PreparedBody::Empty,
        RequestBody::Raw { language, text } => PreparedBody::Bytes {
            data: text.as_bytes().to_vec(),
            content_type: language.content_type(),
        },
        RequestBody::UrlEncoded { fields } => {
            let mut ser = url::form_urlencoded::Serializer::new(String::new());
            for kv in fields.iter().filter(|kv| kv.is_active()) {
                ser.append_pair(&kv.key, &kv.value);
            }
            PreparedBody::Bytes {
                data: ser.finish().into_bytes(),
                content_type: URLENCODED,
            }
        }
        RequestBody::FormData { fields } => {
            let mut form = Form::new();
            for f in fields
                .iter()
                .filter(|f| f.enabled && !f.key.trim().is_empty())
            {
                form = match f.kind {
                    FormFieldKind::Text => form.text(f.key.clone(), f.value.clone()),
                    FormFieldKind::File => form.part(f.key.clone(), file_part(&f.value).await?),
                };
            }
            PreparedBody::Multipart(form)
        }
        RequestBody::Binary { path } => match path.as_deref().map(str::trim) {
            None | Some("") => PreparedBody::Empty,
            Some(path) => PreparedBody::Bytes {
                data: read_file(path).await?,
                content_type: OCTET_STREAM,
            },
        },
    })
}

async fn read_file(path: &str) -> Result<Vec<u8>, ExecError> {
    if path.trim().is_empty() {
        return Err(ExecError::File("no file selected".into()));
    }
    tokio::fs::read(path)
        .await
        .map_err(|e| ExecError::File(format!("{path}: {e}")))
}

async fn file_part(path: &str) -> Result<Part, ExecError> {
    let data = read_file(path).await?;
    let file_name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Part::bytes(data)
        .file_name(file_name)
        .mime_str(mime.essence_str())
        .map_err(|_| ExecError::InvalidRequest(format!("invalid MIME type for {path}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumapi_domain::{KeyValue, RawLanguage};

    #[tokio::test]
    async fn raw_and_urlencoded() {
        let raw = RequestBody::Raw {
            language: RawLanguage::Xml,
            text: "<a/>".into(),
        };
        let PreparedBody::Bytes { data, content_type } = prepare_body(&raw).await.expect("raw")
        else {
            panic!("expected bytes");
        };
        assert_eq!(
            (data.as_slice(), content_type),
            (&b"<a/>"[..], "application/xml")
        );
        let mut off = KeyValue::new("off", "1");
        off.enabled = false;
        let form = RequestBody::UrlEncoded {
            fields: vec![KeyValue::new("a b", "c&d"), off, KeyValue::new("", "x")],
        };
        let PreparedBody::Bytes { data, .. } = prepare_body(&form).await.expect("form") else {
            panic!("expected bytes");
        };
        assert_eq!(data, b"a+b=c%26d");
    }

    #[tokio::test]
    async fn missing_file_is_file_error() {
        let body = RequestBody::Binary {
            path: Some("/definitely/not/here.bin".into()),
        };
        let Err(ExecError::File(msg)) = prepare_body(&body).await else {
            panic!("expected file error");
        };
        assert!(msg.starts_with("/definitely/not/here.bin: "));
        let empty = RequestBody::Binary { path: None };
        assert!(matches!(
            prepare_body(&empty).await,
            Ok(PreparedBody::Empty)
        ));
    }
}
