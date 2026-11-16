//! Automatic layout of an entity-relationship diagram. Tables that hold
//! foreign keys are placed right of the tables they reference (one column
//! per dependency depth, ordered by the barycenter of their neighbours to
//! limit crossings); tables without any relationship go in a grid below.
//! Units are pixels at 100 % zoom; the UI scales and pans them.

use ferrumdata_domain::SchemaSnapshot;
use std::collections::HashMap;

pub const HEADER_HEIGHT: f32 = 28.0;
pub const ROW_HEIGHT: f32 = 20.0;
/// Approximate advance of one monospace character at 12 px.
const CHAR_WIDTH: f32 = 7.3;
const PADDING: f32 = 12.0;
const MIN_WIDTH: f32 = 150.0;
const MAX_WIDTH: f32 = 380.0;
const LAYER_GAP: f32 = 90.0;
const NODE_GAP: f32 = 28.0;
/// Columns listed per table; the rest is summarised in a "… more" row.
pub const MAX_ROWS: usize = 30;
const SWEEPS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErRow {
    pub name: String,
    pub data_type: String,
    pub primary_key: bool,
    pub foreign_key: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ErNode {
    /// Index into the snapshot's tables.
    pub table: usize,
    pub title: String,
    pub rows: Vec<ErRow>,
    /// Columns not listed (past [`MAX_ROWS`]).
    pub hidden: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ErNode {
    /// Vertical centre of row `row` (the header when `None` or hidden).
    #[must_use]
    pub fn row_center(&self, row: Option<usize>) -> f32 {
        match row.filter(|r| *r < self.rows.len()) {
            Some(r) => self.y + HEADER_HEIGHT + ROW_HEIGHT * (r as f32 + 0.5),
            None => self.y + HEADER_HEIGHT / 2.0,
        }
    }
}

/// A foreign key from a column of `from` to a column of `to` (node indexes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErEdge {
    pub from: usize,
    pub from_row: Option<usize>,
    pub to: usize,
    pub to_row: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ErLayout {
    pub nodes: Vec<ErNode>,
    pub edges: Vec<ErEdge>,
    pub width: f32,
    pub height: f32,
}

#[must_use]
pub fn layout(snapshot: &SchemaSnapshot) -> ErLayout {
    let qualified = snapshot.tables.iter().any(|t| {
        t.info.table.schema.is_some() && t.info.table.schema != snapshot.tables[0].info.table.schema
    });
    let index: HashMap<_, usize> = snapshot
        .tables
        .iter()
        .enumerate()
        .map(|(i, t)| (&t.info.table, i))
        .collect();
    let mut nodes: Vec<ErNode> = snapshot
        .tables
        .iter()
        .enumerate()
        .map(|(i, t)| node(i, t, qualified))
        .collect();

    let mut edges = Vec::new();
    for (from, t) in snapshot.tables.iter().enumerate() {
        for fk in &t.details.foreign_keys {
            let Some(&to) = index.get(&fk.ref_table) else {
                continue;
            };
            let row_of = |node: &ErNode, column: Option<&String>| {
                column.and_then(|c| node.rows.iter().position(|r| &r.name == c))
            };
            edges.push(ErEdge {
                from,
                from_row: row_of(&nodes[from], fk.columns.first()),
                to,
                to_row: row_of(&nodes[to], fk.ref_columns.first()),
            });
        }
    }

    let n = nodes.len();
    let mut parents: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in edges.iter().filter(|e| e.from != e.to) {
        parents[e.from].push(e.to);
        children[e.to].push(e.from);
    }
    let connected: Vec<bool> = (0..n)
        .map(|i| !parents[i].is_empty() || !children[i].is_empty())
        .collect();
    let depth = depths(&parents);

    // Connected tables: one column per depth.
    let layers_count = (0..n)
        .filter(|&i| connected[i])
        .map(|i| depth[i] + 1)
        .max()
        .unwrap_or(0);
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); layers_count];
    for i in (0..n).filter(|&i| connected[i]) {
        layers[depth[i]].push(i);
    }
    order_layers(&mut layers, &parents, &children);

    let heights: Vec<f32> = layers
        .iter()
        .map(|l| column_height(l.iter().map(|&i| nodes[i].height)))
        .collect();
    let tallest = heights.iter().copied().fold(0.0, f32::max);
    let mut x = 0.0;
    for (layer, height) in layers.iter().zip(&heights) {
        let mut y = (tallest - height) / 2.0;
        let width = layer.iter().map(|&i| nodes[i].width).fold(0.0, f32::max);
        for &i in layer {
            nodes[i].x = x;
            nodes[i].y = y;
            y += nodes[i].height + NODE_GAP;
        }
        x += width + LAYER_GAP;
    }
    let mut total_width = (x - LAYER_GAP).max(0.0);

    // Unrelated tables: a grid under the connected block.
    let loose: Vec<usize> = (0..n).filter(|&i| !connected[i]).collect();
    let mut total_height = tallest;
    if !loose.is_empty() {
        let per_row = (loose.len() as f32).sqrt().ceil().max(1.0) as usize;
        let mut y = if tallest > 0.0 {
            tallest + LAYER_GAP
        } else {
            0.0
        };
        for row in loose.chunks(per_row) {
            let mut x = 0.0;
            for &i in row {
                nodes[i].x = x;
                nodes[i].y = y;
                x += nodes[i].width + NODE_GAP;
            }
            total_width = total_width.max(x - NODE_GAP);
            y += row.iter().map(|&i| nodes[i].height).fold(0.0, f32::max) + NODE_GAP;
        }
        total_height = y - NODE_GAP;
    }

    ErLayout {
        nodes,
        edges,
        width: total_width,
        height: total_height,
    }
}

fn node(table: usize, t: &ferrumdata_domain::TableSchema, qualified: bool) -> ErNode {
    let title = match (&t.info.table.schema, qualified) {
        (Some(s), true) => format!("{s}.{}", t.info.table.name),
        _ => t.info.table.name.clone(),
    };
    let fk_columns: Vec<&String> = t
        .details
        .foreign_keys
        .iter()
        .flat_map(|f| &f.columns)
        .collect();
    let rows: Vec<ErRow> = t
        .details
        .columns
        .iter()
        .take(MAX_ROWS)
        .map(|c| ErRow {
            name: c.name.clone(),
            data_type: c.data_type.clone(),
            primary_key: c.primary_key || t.info.primary_key.contains(&c.name),
            foreign_key: fk_columns.contains(&&c.name),
        })
        .collect();
    let hidden = t.details.columns.len().saturating_sub(MAX_ROWS);
    let chars = rows
        .iter()
        .map(|r| r.name.chars().count() + r.data_type.chars().count() + 5)
        .chain([title.chars().count() + 2])
        .max()
        .unwrap_or(0);
    let listed = rows.len() + usize::from(hidden > 0);
    ErNode {
        table,
        title,
        rows,
        hidden,
        x: 0.0,
        y: 0.0,
        width: (chars as f32 * CHAR_WIDTH + 2.0 * PADDING).clamp(MIN_WIDTH, MAX_WIDTH),
        height: HEADER_HEIGHT + ROW_HEIGHT * listed.max(1) as f32 + 6.0,
    }
}

/// Longest chain of references to a root table; references back into the
/// current path (cycles) are ignored.
fn depths(parents: &[Vec<usize>]) -> Vec<usize> {
    fn visit(
        i: usize,
        parents: &[Vec<usize>],
        depth: &mut [Option<usize>],
        path: &mut [bool],
    ) -> usize {
        if let Some(d) = depth[i] {
            return d;
        }
        path[i] = true;
        let mut d = 0;
        for &p in &parents[i] {
            if !path[p] {
                d = d.max(visit(p, parents, depth, path) + 1);
            }
        }
        path[i] = false;
        depth[i] = Some(d);
        d
    }
    let mut depth = vec![None; parents.len()];
    let mut path = vec![false; parents.len()];
    (0..parents.len())
        .map(|i| visit(i, parents, &mut depth, &mut path))
        .collect()
}

/// Barycenter sweeps: each table moves towards the average position of its
/// neighbours in the adjacent columns.
fn order_layers(layers: &mut [Vec<usize>], parents: &[Vec<usize>], children: &[Vec<usize>]) {
    let mut position = vec![0.0f32; parents.len()];
    for layer in layers.iter() {
        for (p, &i) in layer.iter().enumerate() {
            position[i] = p as f32;
        }
    }
    for sweep in 0..SWEEPS {
        let down = sweep % 2 == 0;
        let order: Vec<usize> = if down {
            (1..layers.len()).collect()
        } else {
            (0..layers.len().saturating_sub(1)).rev().collect()
        };
        for l in order {
            let neighbours = if down { parents } else { children };
            let key = |i: usize| {
                let n = &neighbours[i];
                if n.is_empty() {
                    position[i]
                } else {
                    n.iter().map(|&j| position[j]).sum::<f32>() / n.len() as f32
                }
            };
            let mut keyed: Vec<(f32, usize)> = layers[l].iter().map(|&i| (key(i), i)).collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            layers[l] = keyed.into_iter().map(|(_, i)| i).collect();
            for (p, &i) in layers[l].iter().enumerate() {
                position[i] = p as f32;
            }
        }
    }
}

fn column_height(heights: impl Iterator<Item = f32>) -> f32 {
    let (sum, count) = heights.fold((0.0, 0), |(s, c), h| (s + h, c + 1));
    sum + NODE_GAP * (count.max(1) - 1) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::{
        ColumnInfo, ForeignKeyInfo, TableDetails, TableInfo, TableKind, TableRef, TableSchema,
    };

    fn table(name: &str, refs: &[&str]) -> TableSchema {
        let column = |name: &str, pk: bool| ColumnInfo {
            name: name.into(),
            data_type: "int".into(),
            nullable: !pk,
            default: None,
            primary_key: pk,
        };
        let mut columns = vec![column("id", true)];
        let foreign_keys = refs
            .iter()
            .map(|r| {
                let col = format!("{r}_id");
                columns.push(column(&col, false));
                ForeignKeyInfo {
                    name: format!("{name}_{r}"),
                    columns: vec![col],
                    ref_table: TableRef {
                        schema: None,
                        name: (*r).into(),
                    },
                    ref_columns: vec!["id".into()],
                    on_delete: "NO ACTION".into(),
                    on_update: "NO ACTION".into(),
                }
            })
            .collect();
        TableSchema {
            info: TableInfo {
                table: TableRef {
                    schema: None,
                    name: name.into(),
                },
                kind: TableKind::Table,
                primary_key: vec!["id".into()],
                estimated_rows: None,
            },
            details: TableDetails {
                columns,
                indexes: Vec::new(),
                foreign_keys,
            },
        }
    }

    fn snapshot(tables: Vec<TableSchema>) -> SchemaSnapshot {
        SchemaSnapshot { tables, omitted: 0 }
    }

    fn overlaps(a: &ErNode, b: &ErNode) -> bool {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    }

    #[test]
    fn referencing_tables_sit_right_of_their_parents_without_overlap() {
        let l = layout(&snapshot(vec![
            table("line", &["orders", "product"]),
            table("orders", &["customer"]),
            table("customer", &[]),
            table("product", &[]),
            table("audit", &[]),
        ]));
        let x = |name: &str| l.nodes.iter().find(|n| n.title == name).map(|n| n.x);
        assert!(x("customer") < x("orders"));
        assert!(x("orders") < x("line"));
        assert!(x("product") < x("line"));
        assert_eq!(l.edges.len(), 3);
        let edge = l.edges.iter().find(|e| e.from == 1).expect("orders edge");
        assert_eq!((edge.to, edge.from_row, edge.to_row), (2, Some(1), Some(0)));
        // The unrelated table goes below everything else.
        let audit = &l.nodes[4];
        assert!(l.nodes[..4].iter().all(|n| audit.y > n.y + n.height));
        for (i, a) in l.nodes.iter().enumerate() {
            for b in &l.nodes[i + 1..] {
                assert!(!overlaps(a, b), "{} overlaps {}", a.title, b.title);
            }
            assert!(a.x + a.width <= l.width + 0.01 && a.y + a.height <= l.height + 0.01);
        }
    }

    #[test]
    fn cycles_and_self_references_do_not_loop() {
        let l = layout(&snapshot(vec![
            table("a", &["b"]),
            table("b", &["a"]),
            table("tree", &["tree"]),
        ]));
        assert_eq!(l.edges.len(), 3);
        assert_ne!(l.nodes[0].x, l.nodes[1].x);
        // A self-reference alone does not make a table "connected".
        assert!(l.nodes[2].y > l.nodes[0].y);
    }

    #[test]
    fn long_tables_are_cut_and_sized() {
        let mut t = table("wide", &[]);
        for i in 0..40 {
            t.details.columns.push(ColumnInfo {
                name: format!("c{i}"),
                data_type: "text".into(),
                nullable: true,
                default: None,
                primary_key: false,
            });
        }
        let l = layout(&snapshot(vec![t]));
        let node = &l.nodes[0];
        assert_eq!(node.rows.len(), MAX_ROWS);
        assert_eq!(node.hidden, 11);
        assert!(node.rows[0].primary_key);
        assert_eq!(node.row_center(Some(0)), HEADER_HEIGHT + ROW_HEIGHT / 2.0);
        assert!(layout(&SchemaSnapshot::default()).nodes.is_empty());
    }
}
