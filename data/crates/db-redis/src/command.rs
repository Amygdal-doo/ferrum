//! Console input: redis-cli style argument splitting and the read-only
//! allow-list.
//!
//! The allow-list is explicit: a command (or a `CONTAINER SUBCOMMAND` pair)
//! that is not listed is treated as a write, so new or unknown commands are
//! refused on read-only profiles. This is a UX guard, not a security
//! boundary: use an ACL user without write permissions for real protection.

/// Plain commands that never modify data or server state.
const READ_COMMANDS: &[&str] = &[
    // Keys
    "EXISTS",
    "TYPE",
    "TTL",
    "PTTL",
    "EXPIRETIME",
    "PEXPIRETIME",
    "SCAN",
    "KEYS",
    "DBSIZE",
    "RANDOMKEY",
    "DUMP",
    "SORT_RO",
    // Strings and bits
    "GET",
    "MGET",
    "GETRANGE",
    "SUBSTR",
    "STRLEN",
    "LCS",
    "GETBIT",
    "BITCOUNT",
    "BITPOS",
    "BITFIELD_RO",
    // Hashes
    "HGET",
    "HMGET",
    "HGETALL",
    "HKEYS",
    "HVALS",
    "HLEN",
    "HEXISTS",
    "HSTRLEN",
    "HRANDFIELD",
    "HSCAN",
    "HTTL",
    "HPTTL",
    "HEXPIRETIME",
    "HPEXPIRETIME",
    // Lists
    "LRANGE",
    "LINDEX",
    "LLEN",
    "LPOS",
    // Sets
    "SMEMBERS",
    "SISMEMBER",
    "SMISMEMBER",
    "SCARD",
    "SRANDMEMBER",
    "SINTER",
    "SUNION",
    "SDIFF",
    "SINTERCARD",
    "SSCAN",
    // Sorted sets
    "ZRANGE",
    "ZRANGEBYSCORE",
    "ZREVRANGE",
    "ZREVRANGEBYSCORE",
    "ZRANGEBYLEX",
    "ZREVRANGEBYLEX",
    "ZSCORE",
    "ZMSCORE",
    "ZRANK",
    "ZREVRANK",
    "ZCARD",
    "ZCOUNT",
    "ZLEXCOUNT",
    "ZRANDMEMBER",
    "ZINTER",
    "ZUNION",
    "ZDIFF",
    "ZINTERCARD",
    "ZSCAN",
    // Streams (XREAD without a consumer group does not acknowledge anything)
    "XRANGE",
    "XREVRANGE",
    "XLEN",
    "XREAD",
    "XPENDING",
    // HyperLogLog and geo
    "PFCOUNT",
    "GEOPOS",
    "GEODIST",
    "GEOHASH",
    "GEOSEARCH",
    "GEORADIUS_RO",
    "GEORADIUSBYMEMBER_RO",
    // Read-only scripts (the server enforces that they do not write)
    "EVAL_RO",
    "EVALSHA_RO",
    "FCALL_RO",
    // RedisJSON reads
    "JSON.GET",
    "JSON.MGET",
    "JSON.TYPE",
    "JSON.STRLEN",
    "JSON.OBJKEYS",
    "JSON.OBJLEN",
    "JSON.ARRLEN",
    "JSON.ARRINDEX",
    // Server
    "PING",
    "ECHO",
    "INFO",
    "TIME",
    "LASTSAVE",
    "ROLE",
    "LOLWUT",
];

/// Container commands whose listed subcommands are reads.
const READ_SUBCOMMANDS: &[(&str, &[&str])] = &[
    ("CONFIG", &["GET", "HELP"]),
    (
        "CLIENT",
        &[
            "LIST",
            "INFO",
            "GETNAME",
            "ID",
            "TRACKINGINFO",
            "GETREDIR",
            "HELP",
        ],
    ),
    (
        "OBJECT",
        &["ENCODING", "FREQ", "IDLETIME", "REFCOUNT", "HELP"],
    ),
    (
        "MEMORY",
        &["USAGE", "STATS", "DOCTOR", "MALLOC-STATS", "HELP"],
    ),
    (
        "COMMAND",
        &[
            "COUNT",
            "DOCS",
            "INFO",
            "LIST",
            "GETKEYS",
            "GETKEYSANDFLAGS",
            "HELP",
        ],
    ),
    ("SLOWLOG", &["GET", "LEN", "HELP"]),
    (
        "LATENCY",
        &["LATEST", "HISTORY", "DOCTOR", "GRAPH", "HISTOGRAM", "HELP"],
    ),
    ("XINFO", &["STREAM", "GROUPS", "CONSUMERS", "HELP"]),
    (
        "ACL",
        &[
            "WHOAMI", "CAT", "LIST", "USERS", "GETUSER", "DRYRUN", "HELP",
        ],
    ),
    (
        "PUBSUB",
        &[
            "CHANNELS",
            "NUMSUB",
            "NUMPAT",
            "SHARDCHANNELS",
            "SHARDNUMSUB",
            "HELP",
        ],
    ),
    (
        "CLUSTER",
        &[
            "INFO",
            "NODES",
            "SLOTS",
            "SHARDS",
            "MYID",
            "KEYSLOT",
            "COUNTKEYSINSLOT",
            "GETKEYSINSLOT",
            "REPLICAS",
            "LINKS",
            "HELP",
        ],
    ),
    ("MODULE", &["LIST", "HELP"]),
    ("FUNCTION", &["LIST", "STATS", "DUMP", "HELP"]),
    ("SCRIPT", &["EXISTS", "HELP"]),
];

/// How the console treats a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandClass {
    /// On the read-only allow-list.
    Read,
    /// Anything else: refused on read-only profiles.
    Write,
    /// Never run from the console, with the reason shown to the user.
    Refused(&'static str),
}

/// Upper-cased command name (and subcommand for containers), for messages.
#[must_use]
pub fn command_name(args: &[Vec<u8>]) -> String {
    let word = |i: usize| {
        args.get(i)
            .map(|a| String::from_utf8_lossy(a).to_ascii_uppercase())
    };
    let Some(name) = word(0) else {
        return String::new();
    };
    match word(1) {
        Some(sub) if READ_SUBCOMMANDS.iter().any(|(c, _)| *c == name) => format!("{name} {sub}"),
        _ => name,
    }
}

#[must_use]
pub fn classify(args: &[Vec<u8>]) -> CommandClass {
    let upper = |i: usize| {
        args.get(i)
            .map(|a| String::from_utf8_lossy(a).to_ascii_uppercase())
    };
    let Some(name) = upper(0) else {
        return CommandClass::Refused("Type a Redis command");
    };
    match name.as_str() {
        "SUBSCRIBE" | "PSUBSCRIBE" | "SSUBSCRIBE" | "UNSUBSCRIBE" | "PUNSUBSCRIBE"
        | "SUNSUBSCRIBE" | "MONITOR" | "SYNC" | "PSYNC" | "REPLCONF" => {
            return CommandClass::Refused(
                "Streaming commands (subscribe, monitor, replication) are not supported in the console",
            );
        }
        "SELECT" | "SWAPDB" => {
            return CommandClass::Refused(
                "Change the DB index in the connection profile instead of SELECT",
            );
        }
        "MULTI" | "EXEC" | "DISCARD" | "WATCH" | "UNWATCH" => {
            return CommandClass::Refused(
                "Transactions are not supported: each console run uses its own connection",
            );
        }
        "AUTH" | "HELLO" | "QUIT" | "RESET" => {
            return CommandClass::Refused(
                "Set credentials and protocol in the connection profile, not in the console",
            );
        }
        _ => {}
    }
    if READ_COMMANDS.contains(&name.as_str()) {
        return CommandClass::Read;
    }
    // `COMMAND` alone lists every command.
    if name == "COMMAND" && args.len() == 1 {
        return CommandClass::Read;
    }
    let read_sub = READ_SUBCOMMANDS
        .iter()
        .find(|(c, _)| *c == name)
        .zip(upper(1))
        .is_some_and(|((_, subs), sub)| subs.contains(&sub.as_str()));
    if read_sub {
        CommandClass::Read
    } else {
        CommandClass::Write
    }
}

/// True if console text parses to a command outside the read allow-list
/// (used by the production confirmation). Unparseable or refused input is
/// not a write: it never runs.
#[must_use]
pub fn writes(text: &str) -> bool {
    split_args(text).is_ok_and(|args| classify(&args) == CommandClass::Write)
}

/// Splits one console line into arguments like redis-cli: whitespace
/// separated, `"double quoted"` with `\n \r \t \b \a \\ \" \xHH` escapes, or
/// `'single quoted'` with only `\'`. A closing quote must end the argument.
pub fn split_args(line: &str) -> Result<Vec<Vec<u8>>, String> {
    let bytes = line.as_bytes();
    let mut args = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let mut arg = Vec::new();
        match bytes[i] {
            b'"' => {
                i += 1;
                loop {
                    let Some(&c) = bytes.get(i) else {
                        return Err("unbalanced double quote".into());
                    };
                    i += 1;
                    match c {
                        b'"' => break,
                        b'\\' => {
                            let Some(&e) = bytes.get(i) else {
                                return Err("unbalanced double quote".into());
                            };
                            i += 1;
                            match e {
                                b'n' => arg.push(b'\n'),
                                b'r' => arg.push(b'\r'),
                                b't' => arg.push(b'\t'),
                                b'b' => arg.push(0x08),
                                b'a' => arg.push(0x07),
                                b'x' => match bytes.get(i..i + 2).and_then(hex_byte) {
                                    Some(b) => {
                                        arg.push(b);
                                        i += 2;
                                    }
                                    None => arg.push(b'x'),
                                },
                                other => arg.push(other),
                            }
                        }
                        other => arg.push(other),
                    }
                }
                ensure_separator(bytes, i)?;
            }
            b'\'' => {
                i += 1;
                loop {
                    let Some(&c) = bytes.get(i) else {
                        return Err("unbalanced single quote".into());
                    };
                    i += 1;
                    match c {
                        b'\'' => break,
                        b'\\' if bytes.get(i) == Some(&b'\'') => {
                            arg.push(b'\'');
                            i += 1;
                        }
                        other => arg.push(other),
                    }
                }
                ensure_separator(bytes, i)?;
            }
            _ => {
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    arg.push(bytes[i]);
                    i += 1;
                }
            }
        }
        args.push(arg);
    }
    Ok(args)
}

fn ensure_separator(bytes: &[u8], i: usize) -> Result<(), String> {
    match bytes.get(i) {
        None => Ok(()),
        Some(c) if c.is_ascii_whitespace() => Ok(()),
        Some(_) => Err("a closing quote must be followed by a space".into()),
    }
}

fn hex_byte(pair: &[u8]) -> Option<u8> {
    let s = std::str::from_utf8(pair).ok()?;
    u8::from_str_radix(s, 16).ok()
}

/// How a reply is laid out in the grid, decided from the command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyShape {
    /// Scalars as one `value` cell; arrays as `#` / `value` rows.
    Plain,
    /// A flat array of alternating pairs (RESP2 maps), with column names.
    Pairs(&'static str, &'static str),
    /// `INFO` text, split into section / field / value rows.
    Info,
}

#[must_use]
pub fn reply_shape(args: &[Vec<u8>]) -> ReplyShape {
    let upper: Vec<String> = args
        .iter()
        .map(|a| String::from_utf8_lossy(a).to_ascii_uppercase())
        .collect();
    let has = |flag: &str| upper.iter().skip(1).any(|a| a == flag);
    match upper.first().map(String::as_str) {
        Some("INFO") => ReplyShape::Info,
        Some("HGETALL") => ReplyShape::Pairs("field", "value"),
        Some("HRANDFIELD") if has("WITHVALUES") => ReplyShape::Pairs("field", "value"),
        Some("CONFIG") if upper.get(1).is_some_and(|s| s == "GET") => {
            ReplyShape::Pairs("parameter", "value")
        }
        Some(
            "ZRANGE" | "ZREVRANGE" | "ZRANGEBYSCORE" | "ZREVRANGEBYSCORE" | "ZRANDMEMBER"
            | "ZINTER" | "ZUNION" | "ZDIFF" | "ZPOPMIN" | "ZPOPMAX",
        ) if has("WITHSCORES") || matches!(upper[0].as_str(), "ZPOPMIN" | "ZPOPMAX") => {
            ReplyShape::Pairs("member", "score")
        }
        _ => ReplyShape::Plain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        split_args(line)
            .expect("parse")
            .into_iter()
            .map(|a| String::from_utf8(a).expect("utf8"))
            .collect()
    }

    #[test]
    fn splits_like_redis_cli() {
        assert_eq!(args("  SET  user:1   hello "), ["SET", "user:1", "hello"]);
        assert_eq!(
            args(r#"SET k "a b\n\"c\" \x41""#),
            ["SET", "k", "a b\n\"c\" A"]
        );
        assert_eq!(args(r"SET k 'it\'s \n'"), ["SET", "k", "it's \\n"]);
        assert_eq!(args(r#"SET k """#), ["SET", "k", ""]);
        assert!(args("").is_empty());
        assert_eq!(
            split_args(r#"SET k "\xff""#).expect("bytes")[2],
            vec![0xff_u8]
        );
        assert!(split_args(r#"GET "open"#).is_err());
        assert!(split_args("GET 'open").is_err());
        assert!(split_args(r#"GET "a"b"#).is_err());
    }

    fn class(line: &str) -> CommandClass {
        classify(&split_args(line).expect("parse"))
    }

    #[test]
    fn allow_list_decides_reads_and_writes() {
        for read in [
            "get k",
            "HGETALL h",
            "scan 0 match user:* count 100",
            "CONFIG GET maxmemory",
            "client list",
            "MEMORY USAGE k",
            "xinfo stream s",
            "COMMAND",
            "info memory",
        ] {
            assert_eq!(class(read), CommandClass::Read, "{read}");
        }
        for write in [
            "SET k v",
            "del k",
            "FLUSHALL",
            "CONFIG SET maxmemory 1",
            "CLIENT KILL ID 1",
            "EXPIRE k 10",
            "EVAL \"return 1\" 0",
            "SORT k STORE x",
            "XREADGROUP GROUP g c STREAMS s >",
            "SOMEFUTURECOMMAND",
            "CONFIG",
        ] {
            assert_eq!(class(write), CommandClass::Write, "{write}");
        }
        assert!(matches!(class("SUBSCRIBE ch"), CommandClass::Refused(_)));
        assert!(matches!(class("select 2"), CommandClass::Refused(_)));
        assert!(matches!(class("MULTI"), CommandClass::Refused(_)));
        assert!(matches!(class(""), CommandClass::Refused(_)));
    }

    #[test]
    fn writes_ignores_unparseable_and_refused_input() {
        assert!(writes("DEL k"));
        assert!(!writes("GET del"));
        assert!(!writes("GET \"open"));
        assert!(!writes("MONITOR"));
        assert!(!writes(""));
    }

    #[test]
    fn names_include_container_subcommands() {
        let a = split_args("config set x 1").expect("parse");
        assert_eq!(command_name(&a), "CONFIG SET");
        assert_eq!(command_name(&split_args("del k").expect("parse")), "DEL");
    }

    #[test]
    fn reply_shapes_from_the_command() {
        let shape = |l: &str| reply_shape(&split_args(l).expect("parse"));
        assert_eq!(shape("HGETALL h"), ReplyShape::Pairs("field", "value"));
        assert_eq!(
            shape("zrange z 0 -1 withscores"),
            ReplyShape::Pairs("member", "score")
        );
        assert_eq!(shape("ZRANGE z 0 -1"), ReplyShape::Plain);
        assert_eq!(shape("info"), ReplyShape::Info);
        assert_eq!(
            shape("config get *"),
            ReplyShape::Pairs("parameter", "value")
        );
    }
}
