//! The tar format of the engine's archive endpoints (`GET`/`PUT
//! /containers/{id}/archive`): an incremental reader that turns a byte
//! stream into entries and file data without buffering whole files, and a
//! writer for single-file uploads. Plain ustar plus the extensions the
//! engine emits: GNU long names (`L`/`K`) and PAX headers (`x`).

const BLOCK: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    /// Hard links, devices, FIFOs…
    Other,
}

/// One header of the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TarEntry {
    /// The full name in the archive, e.g. `etc/ssl/certs/` (dirs may end in `/`).
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    /// Permission bits (`0o644`…).
    pub mode: u32,
    /// Seconds since the epoch.
    pub mtime: i64,
    /// Symlink target.
    pub link: Option<String>,
}

/// What [`TarReader::push`] reports, in archive order.
#[derive(Debug, PartialEq, Eq)]
pub enum TarEvent<'a> {
    Entry(TarEntry),
    /// A piece of the data of the last [`TarEvent::Entry`].
    Data(&'a [u8]),
}

/// How the data after a header is consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Body {
    /// Reported as [`TarEvent::Data`].
    Emit,
    /// A GNU long name (`L`) or link (`K`) for the next header.
    LongName,
    LongLink,
    /// PAX records for the next header.
    Pax,
    /// Global PAX headers and anything else: skipped.
    Skip,
}

/// Incremental tar parser: feed it bytes as they arrive.
#[derive(Debug, Default)]
pub struct TarReader {
    buffer: Vec<u8>,
    /// (body kind, data left, padding left) while inside an entry's data.
    body: Option<(Body, u64, usize)>,
    /// Collected long name / link / PAX bytes.
    meta: Vec<u8>,
    next_name: Option<String>,
    next_link: Option<String>,
    next_size: Option<u64>,
    /// Two zero blocks were read.
    pub finished: bool,
    error: Option<String>,
}

fn octal(field: &[u8]) -> Option<u64> {
    // Base-256 (GNU) for big numbers: the high bit of the first byte is set.
    if field.first().is_some_and(|b| b & 0x80 != 0) {
        return Some(
            field[1..]
                .iter()
                .fold(u64::from(field[0] & 0x7f), |n, b| (n << 8) | u64::from(*b)),
        );
    }
    let text: String = field
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| char::from(*b))
        .collect();
    let text = text.trim();
    if text.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(text, 8).ok()
}

fn text(field: &[u8]) -> String {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

fn padding(size: u64) -> usize {
    #[allow(clippy::cast_possible_truncation)]
    let rem = (size % BLOCK as u64) as usize;
    if rem == 0 { 0 } else { BLOCK - rem }
}

/// `path=…`, `linkpath=…` and `size=…` of PAX records (`"<len> key=value\n"`).
fn pax_records(data: &[u8]) -> Vec<(String, String)> {
    let mut records = Vec::new();
    let mut rest = data;
    while !rest.is_empty() {
        let Some(space) = rest.iter().position(|b| *b == b' ') else {
            break;
        };
        let Some(len) = std::str::from_utf8(&rest[..space])
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
        else {
            break;
        };
        if len <= space + 1 || len > rest.len() {
            break;
        }
        let record = &rest[space + 1..len];
        let record = record.strip_suffix(b"\n").unwrap_or(record);
        if let Some(eq) = record.iter().position(|b| *b == b'=') {
            records.push((
                String::from_utf8_lossy(&record[..eq]).into_owned(),
                String::from_utf8_lossy(&record[eq + 1..]).into_owned(),
            ));
        }
        rest = &rest[len..];
    }
    records
}

impl TarReader {
    /// Parses `bytes`, calling `visit` for every header and data piece.
    /// Errors (a corrupt header) stick: later pushes report them again.
    pub fn push(
        &mut self,
        bytes: &[u8],
        visit: &mut dyn FnMut(TarEvent<'_>),
    ) -> Result<(), String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        let mut input = bytes;
        while !input.is_empty() && !self.finished {
            if let Some((body, left, pad)) = self.body {
                if left > 0 {
                    let take = usize::try_from(left).unwrap_or(usize::MAX).min(input.len());
                    let (chunk, rest) = input.split_at(take);
                    match body {
                        Body::Emit => visit(TarEvent::Data(chunk)),
                        Body::LongName | Body::LongLink | Body::Pax => {
                            self.meta.extend_from_slice(chunk);
                        }
                        Body::Skip => {}
                    }
                    input = rest;
                    self.body = Some((body, left - take as u64, pad));
                    continue;
                }
                if pad > 0 {
                    let take = pad.min(input.len());
                    input = &input[take..];
                    self.body = Some((body, 0, pad - take));
                    continue;
                }
                self.finish_body(body);
                self.body = None;
                continue;
            }
            let need = BLOCK - self.buffer.len();
            let take = need.min(input.len());
            self.buffer.extend_from_slice(&input[..take]);
            input = &input[take..];
            if self.buffer.len() == BLOCK {
                let header = std::mem::take(&mut self.buffer);
                if let Err(error) = self.header(&header, visit) {
                    self.error = Some(error.clone());
                    return Err(error);
                }
            }
        }
        // A body that ended exactly at the end of the input.
        if let Some((body, 0, 0)) = self.body {
            self.finish_body(body);
            self.body = None;
        }
        Ok(())
    }

    fn finish_body(&mut self, body: Body) {
        let meta = std::mem::take(&mut self.meta);
        match body {
            Body::LongName => self.next_name = Some(text(&meta)),
            Body::LongLink => self.next_link = Some(text(&meta)),
            Body::Pax => {
                for (key, value) in pax_records(&meta) {
                    match key.as_str() {
                        "path" => self.next_name = Some(value),
                        "linkpath" => self.next_link = Some(value),
                        "size" => self.next_size = value.parse().ok(),
                        _ => {}
                    }
                }
            }
            Body::Emit | Body::Skip => {}
        }
    }

    fn header(&mut self, header: &[u8], visit: &mut dyn FnMut(TarEvent<'_>)) -> Result<(), String> {
        if header.iter().all(|b| *b == 0) {
            // The first of the two closing zero blocks is enough.
            self.finished = true;
            return Ok(());
        }
        let stored = octal(&header[148..156]).ok_or("invalid tar checksum field")?;
        let sum: u64 = header
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    u64::from(*b)
                }
            })
            .sum();
        if sum != stored {
            return Err("corrupt tar header (checksum mismatch)".into());
        }
        let size = octal(&header[124..136]).ok_or("invalid tar size")?;
        let typeflag = header[156];
        let (body, size) = match typeflag {
            b'L' => (Body::LongName, size),
            b'K' => (Body::LongLink, size),
            b'x' => (Body::Pax, size),
            b'g' => (Body::Skip, size),
            _ => {
                let mut name = text(&header[0..100]);
                if &header[257..262] == b"ustar" {
                    let prefix = text(&header[345..500]);
                    if !prefix.is_empty() {
                        name = format!("{prefix}/{name}");
                    }
                }
                let name = self.next_name.take().unwrap_or(name);
                let size = self.next_size.take().unwrap_or(size);
                let kind = match typeflag {
                    b'0' | 0 | b'7' => EntryKind::File,
                    b'5' => EntryKind::Dir,
                    b'2' => EntryKind::Symlink,
                    _ => EntryKind::Other,
                };
                let link = self.next_link.take().or_else(|| {
                    let link = text(&header[157..257]);
                    (!link.is_empty()).then_some(link)
                });
                #[allow(clippy::cast_possible_truncation)]
                let mode = octal(&header[100..108]).unwrap_or(0) as u32 & 0o7777;
                let mtime = i64::try_from(octal(&header[136..148]).unwrap_or(0)).unwrap_or(0);
                visit(TarEvent::Entry(TarEntry {
                    name,
                    kind,
                    size: if kind == EntryKind::File { size } else { 0 },
                    mode,
                    mtime,
                    link: link.filter(|_| kind == EntryKind::Symlink),
                }));
                if kind == EntryKind::File {
                    (Body::Emit, size)
                } else if matches!(typeflag, b'5' | b'2' | b'1') {
                    // Dirs and links carry no data even when a size is set.
                    (Body::Skip, 0)
                } else {
                    (Body::Skip, size)
                }
            }
        };
        self.body = Some((body, size, padding(size)));
        if size == 0 {
            self.finish_body(body);
            self.body = None;
        }
        Ok(())
    }
}

/// One ustar header block (`name` and `link` must fit in 100 bytes).
fn header_block(
    name: &str,
    size: u64,
    mode: u32,
    mtime: i64,
    typeflag: u8,
    link: &str,
) -> [u8; BLOCK] {
    let mut h = [0u8; BLOCK];
    let put = |h: &mut [u8; BLOCK], at: usize, value: &[u8]| {
        h[at..at + value.len()].copy_from_slice(value);
    };
    put(&mut h, 0, name.as_bytes());
    put(&mut h, 100, format!("{mode:07o}\0").as_bytes());
    put(&mut h, 108, b"0000000\0");
    put(&mut h, 116, b"0000000\0");
    put(&mut h, 124, format!("{size:011o}\0").as_bytes());
    put(&mut h, 136, format!("{:011o}\0", mtime.max(0)).as_bytes());
    put(&mut h, 148, b"        ");
    h[156] = typeflag;
    put(&mut h, 157, link.as_bytes());
    put(&mut h, 257, b"ustar\0");
    put(&mut h, 263, b"00");
    let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
    put(&mut h, 148, format!("{sum:06o}\0 ").as_bytes());
    h
}

/// The longest prefix of `name` that fits in `max` bytes.
fn clip(name: &str, max: usize) -> &str {
    let mut end = name.len().min(max);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

/// A PAX record `"<len> key=value\n"`, where `<len>` counts the whole record.
fn pax_record(key: &str, value: &str) -> String {
    let body = format!(" {key}={value}\n");
    let mut len = body.len() + 1;
    while format!("{len}{body}").len() != len {
        len = format!("{len}{body}").len();
    }
    format!("{len}{body}")
}

/// A tar holding one regular file `name` (a bare file name or a relative
/// path; long names get a PAX header).
#[must_use]
pub fn single_file(name: &str, data: &[u8], mode: u32, mtime: i64) -> Vec<u8> {
    let mut tar = Vec::with_capacity(data.len() + 4 * BLOCK);
    append_file(&mut tar, name, data, mode, mtime);
    tar.resize(tar.len() + 2 * BLOCK, 0);
    tar
}

fn append_file(tar: &mut Vec<u8>, name: &str, data: &[u8], mode: u32, mtime: i64) {
    if name.len() > 100 {
        let record = pax_record("path", name);
        let len = record.len() as u64;
        tar.extend_from_slice(&header_block("PaxHeader", len, 0o644, mtime, b'x', ""));
        tar.extend_from_slice(record.as_bytes());
        tar.resize(tar.len() + padding(len), 0);
    }
    let size = data.len() as u64;
    tar.extend_from_slice(&header_block(clip(name, 100), size, mode, mtime, b'0', ""));
    tar.extend_from_slice(data);
    tar.resize(tar.len() + padding(size), 0);
}

/// An archive of `entries`: (name, kind, file data or symlink target).
/// The fake engine serves these from its archive endpoint.
#[must_use]
pub fn build(entries: &[(&str, EntryKind, &[u8])]) -> Vec<u8> {
    const MTIME: i64 = 1_700_000_000;
    let mut tar = Vec::new();
    for (name, kind, data) in entries {
        match kind {
            EntryKind::File => append_file(&mut tar, name, data, 0o644, MTIME),
            EntryKind::Dir => {
                tar.extend_from_slice(&header_block(clip(name, 100), 0, 0o755, MTIME, b'5', ""));
            }
            EntryKind::Symlink | EntryKind::Other => {
                let link = String::from_utf8_lossy(data);
                tar.extend_from_slice(&header_block(
                    clip(name, 100),
                    0,
                    0o777,
                    MTIME,
                    b'2',
                    clip(&link, 100),
                ));
            }
        }
    }
    tar.resize(tar.len() + 2 * BLOCK, 0);
    tar
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses `tar` fed in pieces of `step` bytes: entries with their data.
    fn read(tar: &[u8], step: usize) -> Vec<(TarEntry, Vec<u8>)> {
        let mut reader = TarReader::default();
        let mut out: Vec<(TarEntry, Vec<u8>)> = Vec::new();
        for chunk in tar.chunks(step) {
            reader
                .push(chunk, &mut |event| match event {
                    TarEvent::Entry(e) => out.push((e, Vec::new())),
                    TarEvent::Data(d) => {
                        if let Some(last) = out.last_mut() {
                            last.1.extend_from_slice(d);
                        }
                    }
                })
                .unwrap_or_else(|e| panic!("{e}"));
        }
        assert!(reader.finished);
        out
    }

    #[test]
    fn round_trips_files_dirs_and_links_in_any_chunking() {
        let tar = build(&[
            ("etc/", EntryKind::Dir, b""),
            ("etc/hosts", EntryKind::File, b"127.0.0.1 localhost\n"),
            ("etc/empty", EntryKind::File, b""),
            (
                "etc/localtime",
                EntryKind::Symlink,
                b"/usr/share/zoneinfo/UTC",
            ),
            ("etc/big", EntryKind::File, &[7u8; 1300]),
        ]);
        for step in [1, 7, 511, 512, 513, 4096] {
            let entries = read(&tar, step);
            let names: Vec<&str> = entries.iter().map(|(e, _)| e.name.as_str()).collect();
            assert_eq!(
                names,
                ["etc/", "etc/hosts", "etc/empty", "etc/localtime", "etc/big"],
                "step {step}"
            );
            assert_eq!(entries[1].1, b"127.0.0.1 localhost\n");
            assert_eq!(entries[1].0.size, 20);
            assert_eq!(entries[1].0.mode, 0o644);
            assert_eq!(entries[0].0.kind, EntryKind::Dir);
            assert_eq!(
                entries[3].0.link.as_deref(),
                Some("/usr/share/zoneinfo/UTC")
            );
            assert_eq!(entries[4].1.len(), 1300);
        }
    }

    #[test]
    fn reads_long_names_from_pax_and_gnu_headers() {
        let long = format!("srv/{}/file.txt", "d".repeat(120));
        let tar = single_file(&long, b"hi", 0o600, 0);
        let entries = read(&tar, 100);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0.name, long);
        assert_eq!(entries[0].0.mode, 0o600);
        assert_eq!(entries[0].1, b"hi");

        // GNU: an `L` entry whose data is the next entry's name.
        let gnu_name = format!("{}.log", "n".repeat(150));
        let mut tar =
            header_block("././@LongLink", gnu_name.len() as u64 + 1, 0, 0, b'L', "").to_vec();
        tar.extend_from_slice(gnu_name.as_bytes());
        tar.push(0);
        tar.resize(tar.len() + padding(gnu_name.len() as u64 + 1), 0);
        tar.extend_from_slice(&build(&[("short", EntryKind::File, b"x")]));
        let entries = read(&tar, 64);
        assert_eq!(entries[0].0.name, gnu_name);
        assert_eq!(entries[0].1, b"x");
    }

    #[test]
    fn rejects_corrupt_headers() {
        let mut tar = build(&[("a", EntryKind::File, b"x")]);
        tar[0] = b'b';
        let mut reader = TarReader::default();
        let result = reader.push(&tar, &mut |_| {});
        assert!(result.is_err_and(|e| e.contains("checksum")));
        assert!(reader.push(b"more", &mut |_| {}).is_err());
    }

    #[test]
    fn writes_pax_record_lengths_that_count_themselves() {
        let record = pax_record("path", "abc");
        assert_eq!(record, "12 path=abc\n");
        assert_eq!(record.len(), 12);
        let long = pax_record("path", &"x".repeat(95));
        assert_eq!(
            long.len().to_string(),
            long.split(' ').next().unwrap_or_default()
        );
    }
}
