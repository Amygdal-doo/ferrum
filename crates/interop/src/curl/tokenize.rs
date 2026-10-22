//! A small POSIX-shell-style word splitter: single quotes, double quotes with
//! backslash escapes, `$'...'` ANSI-C quoting, `\`-newline and cmd.exe
//! `^`-newline continuations. No expansion of variables or globs.

use crate::InteropError;
use std::iter::Peekable;
use std::str::Chars;

pub fn tokenize(input: &str) -> Result<Vec<String>, InteropError> {
    let mut words = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut in_word = false;
    let mut it = input.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            c if c.is_whitespace() => {
                if in_word {
                    words.push(String::from_utf8_lossy(&cur).into_owned());
                    cur.clear();
                    in_word = false;
                }
            }
            '\\' | '^' if at_newline(&mut it) => {}
            '\\' => {
                in_word = true;
                if let Some(n) = it.next() {
                    push(&mut cur, n);
                }
            }
            '\'' => {
                in_word = true;
                single(&mut it, &mut cur)?;
            }
            '"' => {
                in_word = true;
                double(&mut it, &mut cur)?;
            }
            '$' if it.peek() == Some(&'\'') => {
                it.next();
                in_word = true;
                ansi_c(&mut it, &mut cur)?;
            }
            '$' if it.peek() == Some(&'"') => {
                it.next();
                in_word = true;
                double(&mut it, &mut cur)?;
            }
            c => {
                in_word = true;
                push(&mut cur, c);
            }
        }
    }
    if in_word {
        words.push(String::from_utf8_lossy(&cur).into_owned());
    }
    Ok(words)
}

/// Consumes `\n` or `\r\n` if it comes next.
fn at_newline(it: &mut Peekable<Chars<'_>>) -> bool {
    match it.peek() {
        Some('\n') => {
            it.next();
            true
        }
        Some('\r') => {
            let mut look = it.clone();
            look.next();
            if look.peek() == Some(&'\n') {
                it.next();
                it.next();
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

fn push(buf: &mut Vec<u8>, c: char) {
    let mut tmp = [0u8; 4];
    buf.extend_from_slice(c.encode_utf8(&mut tmp).as_bytes());
}

fn unterminated(kind: &str) -> InteropError {
    InteropError::Parse(format!("unterminated {kind} quote"))
}

fn single(it: &mut Peekable<Chars<'_>>, cur: &mut Vec<u8>) -> Result<(), InteropError> {
    for c in it.by_ref() {
        if c == '\'' {
            return Ok(());
        }
        push(cur, c);
    }
    Err(unterminated("single"))
}

fn double(it: &mut Peekable<Chars<'_>>, cur: &mut Vec<u8>) -> Result<(), InteropError> {
    while let Some(c) = it.next() {
        match c {
            '"' => return Ok(()),
            '\\' => match it.next() {
                Some('\n') => {}
                Some(n @ ('$' | '`' | '"' | '\\')) => push(cur, n),
                Some(n) => {
                    push(cur, '\\');
                    push(cur, n);
                }
                None => break,
            },
            c => push(cur, c),
        }
    }
    Err(unterminated("double"))
}

/// `$'...'`: escapes are decoded to bytes so `\xHH` sequences can form UTF-8.
fn ansi_c(it: &mut Peekable<Chars<'_>>, cur: &mut Vec<u8>) -> Result<(), InteropError> {
    while let Some(c) = it.next() {
        match c {
            '\'' => return Ok(()),
            '\\' => {
                let Some(e) = it.next() else { break };
                match e {
                    'n' => cur.push(b'\n'),
                    't' => cur.push(b'\t'),
                    'r' => cur.push(b'\r'),
                    'a' => cur.push(0x07),
                    'b' => cur.push(0x08),
                    'e' | 'E' => cur.push(0x1b),
                    'f' => cur.push(0x0c),
                    'v' => cur.push(0x0b),
                    'x' => {
                        let v = digits(it, 16, 2);
                        cur.push(u8::try_from(v).unwrap_or(b'?'));
                    }
                    'u' | 'U' => {
                        let v = digits(it, 16, if e == 'u' { 4 } else { 8 });
                        push(cur, char::from_u32(v).unwrap_or('\u{fffd}'));
                    }
                    '0'..='7' => {
                        let mut v = e.to_digit(8).unwrap_or(0);
                        for _ in 0..2 {
                            match it.peek().and_then(|c| c.to_digit(8)) {
                                Some(d) => {
                                    v = v * 8 + d;
                                    it.next();
                                }
                                None => break,
                            }
                        }
                        cur.push(u8::try_from(v & 0xff).unwrap_or(b'?'));
                    }
                    other => push(cur, other),
                }
            }
            c => push(cur, c),
        }
    }
    Err(unterminated("$'...'"))
}

/// Up to `max` digits in `radix` (fewer if the input stops matching).
fn digits(it: &mut Peekable<Chars<'_>>, radix: u32, max: usize) -> u32 {
    let mut v = 0u32;
    for _ in 0..max {
        match it.peek().and_then(|c| c.to_digit(radix)) {
            Some(d) => {
                v = v * radix + d;
                it.next();
            }
            None => break,
        }
    }
    v
}
