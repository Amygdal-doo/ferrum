//! Requests, responses and events.
//!
//! Targets are strings resolved by the server: an id (`s1`, `w2`, `b3`) or a
//! session name / window name / block label. `None` targets mean "the
//! caller's own block" (`FTERM_BLOCK`) when the CLI fills them in, else the
//! most recently focused block.

use crate::frame::Frame;
use crate::ids::{BlockId, SessionId, WindowId};
use crate::status::ProgramStatus;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Right,
    Down,
    Left,
    Up,
}

impl Direction {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "right" | "r" | "h" | "horizontal" => Some(Self::Right),
            "down" | "d" | "v" | "vertical" => Some(Self::Down),
            "left" | "l" => Some(Self::Left),
            "up" | "u" => Some(Self::Up),
            _ => None,
        }
    }
}

/// Split axis of a layout node: `Row` = side by side, `Column` = stacked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Row,
    Column,
}

/// Binary split tree of blocks within a window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    Block(BlockId),
    Split {
        axis: Axis,
        ratio: f32,
        a: Box<Layout>,
        b: Box<Layout>,
    },
}

/// A block's rectangle in cells within its window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub cols: u16,
    pub rows: u16,
}

impl Layout {
    #[must_use]
    pub fn blocks(&self) -> Vec<BlockId> {
        match self {
            Self::Block(b) => vec![*b],
            Self::Split { a, b, .. } => {
                let mut v = a.blocks();
                v.extend(b.blocks());
                v
            }
        }
    }

    /// Cell rectangles for every block in a `cols`×`rows` area. One cell is
    /// reserved for the divider between the halves of each split.
    #[must_use]
    pub fn rects(&self, area: Rect) -> Vec<(BlockId, Rect)> {
        let mut out = Vec::new();
        self.collect(area, &mut out);
        out
    }

    fn collect(&self, r: Rect, out: &mut Vec<(BlockId, Rect)>) {
        match self {
            Self::Block(b) => out.push((*b, r)),
            Self::Split { axis, ratio, a, b } => {
                let total = match axis {
                    Axis::Row => r.cols,
                    Axis::Column => r.rows,
                };
                let usable = total.saturating_sub(1).max(2);
                let first = ((f32::from(usable) * ratio.clamp(0.05, 0.95)).round() as u16)
                    .clamp(1, usable - 1);
                let second = usable - first;
                let (ra, rb) = match axis {
                    Axis::Row => (
                        Rect { cols: first, ..r },
                        Rect {
                            x: r.x + first + 1,
                            cols: second,
                            ..r
                        },
                    ),
                    Axis::Column => (
                        Rect { rows: first, ..r },
                        Rect {
                            y: r.y + first + 1,
                            rows: second,
                            ..r
                        },
                    ),
                };
                a.collect(ra, out);
                b.collect(rb, out);
            }
        }
    }
}

/// How to start a block's process.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Spawn {
    /// Empty = the user's login shell.
    pub command: Vec<String>,
    pub cwd: Option<String>,
    pub label: Option<String>,
    /// Keep the block (showing its final screen) after the process exits.
    pub keep_open: bool,
    pub env: Vec<(String, String)>,
}

/// Keyboard / paste input for a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    /// Literal text typed as-is.
    Text(String),
    /// A key spec: `Enter`, `Tab`, `Escape`, `Up`, `C-c`, `M-f`, `S-Tab`, `F5`…
    Key(String),
    /// Pasted text (wrapped in bracketed-paste markers when the program asked for it).
    Paste(String),
    /// Raw bytes (base64 not needed: only valid UTF-8 is sent by clients).
    Raw(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AwakeMode {
    /// Keep the machine awake while any block reports `working`.
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Hello {
        client: String,
        token: Option<String>,
        protocol: u32,
    },
    ServerInfo,
    List,
    NewSession {
        name: Option<String>,
        spawn: Spawn,
    },
    NewWindow {
        session: Option<String>,
        name: Option<String>,
        spawn: Spawn,
    },
    Split {
        target: Option<String>,
        dir: Direction,
        ratio: Option<f32>,
        spawn: Spawn,
        focus: bool,
    },
    Input {
        target: Option<String>,
        input: Vec<Input>,
    },
    Capture {
        target: Option<String>,
        scrollback: bool,
    },
    Inspect {
        target: String,
    },
    /// Size of a window's content area in cells (from the client showing it).
    Resize {
        target: String,
        cols: u16,
        rows: u16,
    },
    Focus {
        target: String,
    },
    /// Close a block, window or session.
    Close {
        target: String,
    },
    Rename {
        target: String,
        name: String,
    },
    /// Scroll the viewport: positive = into history; `None` = back to bottom.
    Scroll {
        target: Option<String>,
        lines: Option<i32>,
    },
    SetStatus {
        target: Option<String>,
        status: Option<ProgramStatus>,
    },
    /// Stream frames for these blocks (replaces any previous subscription).
    Subscribe {
        blocks: Vec<BlockId>,
    },
    /// Stream structural / status events.
    Watch,
    /// Wait until the block exits, or (with `status`) until it reports one of these states.
    Wait {
        target: Option<String>,
        status: Vec<crate::ProgramState>,
        timeout_ms: Option<u64>,
    },
    SetAwake {
        mode: AwakeMode,
    },
    /// Start an SSH port forward as a server-owned child process
    /// (`command` is the full `ssh -N …` argv, built by the client).
    ForwardStart {
        host: String,
        spec: crate::ForwardSpec,
        command: Vec<String>,
        /// Extra environment (e.g. `SSH_AUTH_SOCK`, so ssh-agent keys work).
        #[serde(default)]
        env: Vec<(String, String)>,
    },
    /// Stop a forward (`host/L8080`), or clear a failed one.
    ForwardStop {
        key: String,
    },
    ForwardList,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockInfo {
    pub id: BlockId,
    pub label: Option<String>,
    pub title: String,
    pub command: Vec<String>,
    pub cwd: Option<String>,
    pub pid: Option<u32>,
    pub cols: u16,
    pub rows: u16,
    pub status: Option<ProgramStatus>,
    /// Exit code once the process has exited (block kept open).
    pub exited: Option<i32>,
    /// Coding agent in the foreground (`claude`, `codex`…), as the server
    /// sees it: the started program or the terminal's foreground process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: WindowId,
    pub name: String,
    pub layout: Layout,
    pub focused: BlockId,
    pub zoomed: bool,
    pub cols: u16,
    pub rows: u16,
    pub blocks: Vec<BlockInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub name: String,
    pub created_ms: u64,
    pub active_window: WindowId,
    pub windows: Vec<WindowInfo>,
}

impl SessionInfo {
    /// Most urgent program status in the session (blocked > error > working > done > idle).
    #[must_use]
    pub fn summary_status(&self) -> Option<crate::ProgramState> {
        use crate::ProgramState::*;
        let rank = |s: crate::ProgramState| match s {
            Blocked => 5,
            Error => 4,
            Working => 3,
            Done => 2,
            Idle => 1,
        };
        self.windows
            .iter()
            .flat_map(|w| &w.blocks)
            .filter_map(|b| b.status.as_ref().map(|s| s.state))
            .max_by_key(|s| rank(*s))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub version: String,
    pub protocol: u32,
    pub pid: u32,
    pub started_ms: u64,
    pub socket: String,
    pub listeners: Vec<String>,
    pub sessions: usize,
    pub blocks: usize,
    pub clients: usize,
    pub awake_mode: AwakeMode,
    /// A keep-awake assertion is currently held.
    pub awake: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Inspected {
    Session(SessionInfo),
    Window(WindowInfo),
    Block(BlockInfo),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Welcome {
        info: ServerInfo,
    },
    Info {
        info: ServerInfo,
    },
    Sessions {
        sessions: Vec<SessionInfo>,
    },
    Created {
        session: SessionId,
        window: WindowId,
        block: BlockId,
    },
    Text {
        text: String,
    },
    Inspected {
        item: Inspected,
    },
    /// `Wait` finished: exit code, or the status that matched.
    Waited {
        exit: Option<i32>,
        status: Option<ProgramStatus>,
    },
    Forwards {
        forwards: Vec<crate::ForwardInfo>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Frame {
        block: BlockId,
        frame: Frame,
    },
    /// Sessions, windows, layouts, names or focus changed; re-`List`.
    Changed,
    Status {
        block: BlockId,
        status: Option<ProgramStatus>,
    },
    Exited {
        block: BlockId,
        code: i32,
    },
    Bell {
        block: BlockId,
    },
    /// A port forward started, stopped or failed; re-`ForwardList`.
    Forwards,
    /// The server is shutting down.
    Bye,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientMsg {
    pub id: u64,
    #[serde(flatten)]
    pub req: Request,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerMsg {
    Reply { id: u64, res: Response },
    Event(Event),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_rects_reserve_divider() {
        let l = Layout::Split {
            axis: Axis::Row,
            ratio: 0.5,
            a: Box::new(Layout::Block(BlockId(1))),
            b: Box::new(Layout::Split {
                axis: Axis::Column,
                ratio: 0.5,
                a: Box::new(Layout::Block(BlockId(2))),
                b: Box::new(Layout::Block(BlockId(3))),
            }),
        };
        let r = l.rects(Rect {
            x: 0,
            y: 0,
            cols: 81,
            rows: 25,
        });
        assert_eq!(
            r[0],
            (
                BlockId(1),
                Rect {
                    x: 0,
                    y: 0,
                    cols: 40,
                    rows: 25
                }
            )
        );
        assert_eq!(
            r[1],
            (
                BlockId(2),
                Rect {
                    x: 41,
                    y: 0,
                    cols: 40,
                    rows: 12
                }
            )
        );
        assert_eq!(
            r[2],
            (
                BlockId(3),
                Rect {
                    x: 41,
                    y: 13,
                    cols: 40,
                    rows: 12
                }
            )
        );
        assert_eq!(l.blocks(), vec![BlockId(1), BlockId(2), BlockId(3)]);
    }

    #[test]
    fn messages_round_trip_as_json() {
        let m = ClientMsg {
            id: 3,
            req: Request::Input {
                target: Some("b2".into()),
                input: vec![Input::Text("ls".into()), Input::Key("Enter".into())],
            },
        };
        let json = serde_json::to_string(&m).expect("json");
        assert!(json.contains(r#""op":"input""#), "{json}");
        assert_eq!(serde_json::from_str::<ClientMsg>(&json).expect("parse"), m);
        let e = ServerMsg::Event(Event::Exited {
            block: BlockId(2),
            code: 0,
        });
        let json = serde_json::to_string(&e).expect("json");
        assert_eq!(serde_json::from_str::<ServerMsg>(&json).expect("parse"), e);
    }
}
