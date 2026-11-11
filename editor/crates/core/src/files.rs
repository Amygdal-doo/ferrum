//! Loading and saving text files: UTF-8 (with or without BOM) only,
//! binary detection, line-ending detection / restoration, atomic writes.

use std::io;
use std::path::Path;
use std::time::SystemTime;

/// Files larger than this are refused (the editor keeps them in memory).
pub const MAX_FILE_SIZE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    Crlf,
}

impl LineEnding {
    pub fn label(self) -> &'static str {
        match self {
            LineEnding::Lf => "LF",
            LineEnding::Crlf => "CRLF",
        }
    }

    /// The majority line ending of `text` (LF when there are none).
    pub fn detect(text: &str) -> Self {
        let crlf = text.matches("\r\n").count();
        let lf = text.matches('\n').count() - crlf;
        if crlf > lf {
            LineEnding::Crlf
        } else {
            LineEnding::Lf
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedFile {
    /// Text with `\n` line endings and no BOM.
    pub text: String,
    pub line_ending: LineEnding,
    pub bom: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("file is larger than 64 MB")]
    TooLarge,
    #[error("binary file")]
    Binary,
    #[error("file is not valid UTF-8")]
    NotUtf8,
}

pub fn decode(bytes: Vec<u8>) -> Result<LoadedFile, LoadError> {
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Err(LoadError::Binary);
    }
    let (bom, bytes) = match bytes.strip_prefix(b"\xEF\xBB\xBF") {
        Some(rest) => (true, rest.to_vec()),
        None => (false, bytes),
    };
    let text = String::from_utf8(bytes).map_err(|_| LoadError::NotUtf8)?;
    let line_ending = LineEnding::detect(&text);
    let text = crate::buffer::normalize_newlines(&text).into_owned();
    Ok(LoadedFile {
        text,
        line_ending,
        bom,
    })
}

pub fn load(path: &Path) -> Result<LoadedFile, LoadError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(LoadError::TooLarge);
    }
    decode(std::fs::read(path)?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SaveOptions {
    pub line_ending: LineEnding,
    pub bom: bool,
    pub trim_trailing_whitespace: bool,
    pub ensure_final_newline: bool,
}

/// The bytes to write for buffer text `text` (which uses `\n`).
pub fn encode(text: &str, options: SaveOptions) -> Vec<u8> {
    let mut text = if options.trim_trailing_whitespace {
        text.split('\n')
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        text.to_string()
    };
    if options.ensure_final_newline && !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    if options.line_ending == LineEnding::Crlf {
        text = text.replace('\n', "\r\n");
    }
    let mut bytes = Vec::with_capacity(text.len() + 3);
    if options.bom {
        bytes.extend_from_slice(b"\xEF\xBB\xBF");
    }
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

/// Writes via a temporary file in the same folder and renames it over the
/// target, keeping the original file's permissions.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(".{name}.ferrum-tmp"));
    std::fs::write(&tmp, bytes)?;
    if let Ok(metadata) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, metadata.permissions());
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Modification time and size, used to notice changes on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskStamp {
    pub modified: Option<SystemTime>,
    pub len: u64,
}

pub fn disk_stamp(path: &Path) -> Option<DiskStamp> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(DiskStamp {
        modified: metadata.modified().ok(),
        len: metadata.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_line_endings() {
        assert_eq!(LineEnding::detect("a\r\nb\r\nc\n"), LineEnding::Crlf);
        assert_eq!(LineEnding::detect("a\nb\r\n"), LineEnding::Lf);
        assert_eq!(LineEnding::detect("abc"), LineEnding::Lf);
    }

    #[test]
    fn decodes_bom_crlf_and_rejects_binary() {
        let file = decode(b"\xEF\xBB\xBFa\r\nb".to_vec()).expect("text");
        assert_eq!(file.text, "a\nb");
        assert!(file.bom);
        assert_eq!(file.line_ending, LineEnding::Crlf);
        assert!(matches!(decode(b"a\0b".to_vec()), Err(LoadError::Binary)));
        assert!(matches!(
            decode(vec![0xff, 0xfe, 0x41]),
            Err(LoadError::NotUtf8)
        ));
    }

    #[test]
    fn encodes_with_options() {
        let options = SaveOptions {
            line_ending: LineEnding::Crlf,
            bom: true,
            trim_trailing_whitespace: true,
            ensure_final_newline: true,
        };
        assert_eq!(
            encode("a  \nb", options),
            b"\xEF\xBB\xBFa\r\nb\r\n".to_vec()
        );
        assert_eq!(encode("x", SaveOptions::default()), b"x".to_vec());
        let final_nl = SaveOptions {
            ensure_final_newline: true,
            ..SaveOptions::default()
        };
        assert_eq!(encode("", final_nl), b"".to_vec());
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f.txt");
        write_atomic(&path, b"one\r\ntwo").expect("write");
        let loaded = load(&path).expect("load");
        assert_eq!(loaded.text, "one\ntwo");
        let options = SaveOptions {
            line_ending: loaded.line_ending,
            ..SaveOptions::default()
        };
        write_atomic(&path, &encode(&loaded.text, options)).expect("write");
        assert_eq!(std::fs::read(&path).expect("read"), b"one\r\ntwo");
        assert!(disk_stamp(&path).is_some_and(|s| s.len == 8));
        assert_eq!(std::fs::read_dir(dir.path()).expect("dir").count(), 1);
    }
}
