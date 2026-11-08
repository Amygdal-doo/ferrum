//! Commit graph lane layout (GitKraken-style), pure and UI-agnostic.
//!
//! Input: commits ordered children-before-parents (`git log --date-order`).
//! Output: one [`GraphRow`] per commit with the node's lane and the line
//! segments to draw in the row's top half (incoming) and bottom half
//! (outgoing). Lanes keep their position while active — freed slots are
//! reused but never shifted — so branch lines stay straight.

use ferrumgit_domain::{Commit, Oid};
use std::collections::HashMap;

/// A line segment within a row, from lane `from` to lane `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub color: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GraphRow {
    /// Lane of the commit's node.
    pub lane: usize,
    pub color: usize,
    /// From the row's top edge to its middle (lanes entering the row).
    pub up: Vec<Edge>,
    /// From the row's middle to its bottom edge (lanes leaving the row).
    pub down: Vec<Edge>,
    /// Lanes needed to draw this row (max lane index + 1).
    pub width: usize,
}

#[derive(Clone)]
struct Lane {
    /// Next commit expected in this lane.
    oid: Oid,
    color: usize,
}

/// Lays out `commits`. Parents outside the list keep their lane running to
/// the bottom of the last row.
#[must_use]
pub fn layout(commits: &[Commit]) -> Vec<GraphRow> {
    let mut lanes: Vec<Option<Lane>> = Vec::new();
    let mut next_color = 0usize;
    let mut rows = Vec::with_capacity(commits.len());
    let mut new_color = || {
        let c = next_color;
        next_color += 1;
        c
    };

    for commit in commits {
        // Lanes waiting for this commit converge into its node.
        let waiting: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.as_ref().filter(|l| l.oid == commit.oid).map(|_| i))
            .collect();
        let (lane, color) = match waiting.first() {
            Some(&i) => (i, lanes[i].as_ref().map_or(0, |l| l.color)),
            None => {
                let i = free_slot(&lanes);
                (i, new_color())
            }
        };

        let mut row = GraphRow {
            lane,
            color,
            ..GraphRow::default()
        };
        for (i, l) in lanes.iter().enumerate() {
            let Some(l) = l else { continue };
            if waiting.contains(&i) {
                row.up.push(Edge {
                    from: i,
                    to: lane,
                    color: l.color,
                });
            } else {
                row.up.push(Edge {
                    from: i,
                    to: i,
                    color: l.color,
                });
            }
        }
        for &i in &waiting {
            lanes[i] = None;
        }
        ensure(&mut lanes, lane);

        // First parent continues in the node's lane; others join or open lanes.
        let mut parent_lanes: HashMap<usize, usize> = HashMap::new();
        // The first parent always continues straight down, even when another
        // lane already expects it; duplicate lanes converge at the parent's row.
        for (n, parent) in commit.parents.iter().enumerate() {
            if n == 0 {
                lanes[lane] = Some(Lane {
                    oid: parent.clone(),
                    color,
                });
                parent_lanes.insert(lane, color);
            } else if let Some(i) = position(&lanes, parent) {
                parent_lanes.insert(i, lanes[i].as_ref().map_or(0, |l| l.color));
            } else {
                let i = free_slot(&lanes);
                ensure(&mut lanes, i);
                let c = new_color();
                lanes[i] = Some(Lane {
                    oid: parent.clone(),
                    color: c,
                });
                parent_lanes.insert(i, c);
            }
        }
        for (i, l) in lanes.iter().enumerate() {
            let Some(l) = l else { continue };
            match parent_lanes.get(&i) {
                Some(&c) => row.down.push(Edge {
                    from: lane,
                    to: i,
                    color: c,
                }),
                None => row.down.push(Edge {
                    from: i,
                    to: i,
                    color: l.color,
                }),
            }
        }
        while matches!(lanes.last(), Some(None)) {
            lanes.pop();
        }
        row.width = row
            .up
            .iter()
            .chain(&row.down)
            .flat_map(|e| [e.from, e.to])
            .chain([lane])
            .max()
            .map_or(1, |m| m + 1);
        rows.push(row);
    }
    rows
}

fn free_slot(lanes: &[Option<Lane>]) -> usize {
    lanes
        .iter()
        .position(Option::is_none)
        .unwrap_or(lanes.len())
}

fn ensure(lanes: &mut Vec<Option<Lane>>, i: usize) {
    if lanes.len() <= i {
        lanes.resize(i + 1, None);
    }
}

fn position(lanes: &[Option<Lane>], oid: &Oid) -> Option<usize> {
    lanes
        .iter()
        .position(|l| l.as_ref().is_some_and(|l| &l.oid == oid))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(oid: &str, parents: &[&str]) -> Commit {
        Commit {
            oid: Oid::new(oid),
            parents: parents.iter().map(|p| Oid::new(*p)).collect(),
            ..Commit::default()
        }
    }

    #[test]
    fn linear_history_is_one_lane() {
        let rows = layout(&[c("c", &["b"]), c("b", &["a"]), c("a", &[])]);
        assert!(rows.iter().all(|r| r.lane == 0 && r.width == 1));
        assert!(rows[0].up.is_empty(), "tip has nothing above");
        assert_eq!(
            rows[0].down,
            vec![Edge {
                from: 0,
                to: 0,
                color: 0
            }]
        );
        assert!(rows[2].down.is_empty(), "root has nothing below");
    }

    #[test]
    fn branch_and_merge() {
        //  m        merge of b1 into main
        //  |\
        //  | f      feature commit
        //  b |      main commit
        //  |/
        //  a
        let rows = layout(&[
            c("m", &["b", "f"]),
            c("f", &["a"]),
            c("b", &["a"]),
            c("a", &[]),
        ]);
        assert_eq!(rows[0].lane, 0);
        assert_eq!(rows[0].down.len(), 2);
        assert!(rows[0].down.contains(&Edge {
            from: 0,
            to: 1,
            color: 1
        }));
        assert_eq!(rows[1].lane, 1, "feature in its own lane");
        assert_eq!(rows[2].lane, 0);
        // Both lanes converge into a.
        assert_eq!(rows[3].lane, 0);
        assert_eq!(rows[3].up.len(), 2);
        assert!(rows[3].up.contains(&Edge {
            from: 1,
            to: 0,
            color: 1
        }));
        assert_eq!(rows[3].width, 2);
    }

    #[test]
    fn two_tips_get_separate_lanes_and_colors() {
        let rows = layout(&[c("x", &["a"]), c("y", &["a"]), c("a", &[])]);
        assert_eq!((rows[0].lane, rows[1].lane), (0, 1));
        assert_ne!(rows[0].color, rows[1].color);
        assert_eq!(rows[2].up.len(), 2);
    }

    #[test]
    fn freed_lane_is_reused_without_shifting() {
        // Lane 1 closes at "a"; a later unrelated tip reuses it.
        let rows = layout(&[
            c("t1", &["a"]),
            c("t2", &["a"]),
            c("a", &[]),
            c("z", &["y"]),
            c("y", &[]),
        ]);
        assert_eq!(rows[3].lane, 0);
        assert!(rows.iter().all(|r| r.width <= 2));
    }

    #[test]
    fn missing_parents_keep_lane_open() {
        let rows = layout(&[c("b", &["a"])]);
        assert_eq!(
            rows[0].down,
            vec![Edge {
                from: 0,
                to: 0,
                color: 0
            }]
        );
    }
}
