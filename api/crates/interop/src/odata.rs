//! OData `$metadata` (CSDL, OData v2–v4): entity sets with their entity
//! type's keys, properties and navigation properties.

use crate::InteropError;
use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ODataEntitySet {
    pub name: String,
    /// Qualified type name, e.g. `Trippin.Person`.
    pub entity_type: String,
    pub keys: Vec<String>,
    /// `(name, type)`.
    pub properties: Vec<(String, String)>,
    pub navigation: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ODataMetadata {
    /// `Version` of the edmx root (e.g. `4.0`, `1.0` for v2/v3).
    pub version: String,
    pub entity_sets: Vec<ODataEntitySet>,
}

fn local<'a>(n: &Node<'a, '_>) -> &'a str {
    n.tag_name().name()
}

fn named<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children()
        .filter(move |c| c.is_element() && local(c) == name)
}

pub fn parse_metadata(xml: &str) -> Result<ODataMetadata, InteropError> {
    let doc = Document::parse(xml).map_err(|e| InteropError::Parse(format!("$metadata: {e}")))?;
    let root = doc.root_element();
    if local(&root) != "Edmx" {
        return Err(InteropError::Unsupported(
            "not an OData $metadata document (no edmx:Edmx root)".into(),
        ));
    }
    let mut types: HashMap<String, ODataEntitySet> = HashMap::new();
    let mut sets = Vec::new();
    for schema in doc
        .descendants()
        .filter(|n| n.is_element() && local(n) == "Schema")
    {
        let ns = schema.attribute("Namespace").unwrap_or_default();
        let alias = schema.attribute("Alias");
        for et in named(schema, "EntityType") {
            let name = et.attribute("Name").unwrap_or_default();
            let info = ODataEntitySet {
                keys: named(et, "Key")
                    .flat_map(|k| named(k, "PropertyRef"))
                    .filter_map(|p| p.attribute("Name").map(str::to_owned))
                    .collect(),
                properties: named(et, "Property")
                    .map(|p| {
                        (
                            p.attribute("Name").unwrap_or_default().to_owned(),
                            p.attribute("Type").unwrap_or_default().to_owned(),
                        )
                    })
                    .collect(),
                navigation: named(et, "NavigationProperty")
                    .filter_map(|p| p.attribute("Name").map(str::to_owned))
                    .collect(),
                ..ODataEntitySet::default()
            };
            types.insert(format!("{ns}.{name}"), info.clone());
            if let Some(alias) = alias {
                types.insert(format!("{alias}.{name}"), info);
            }
        }
        for container in named(schema, "EntityContainer") {
            for set in named(container, "EntitySet") {
                sets.push(ODataEntitySet {
                    name: set.attribute("Name").unwrap_or_default().to_owned(),
                    entity_type: set.attribute("EntityType").unwrap_or_default().to_owned(),
                    ..ODataEntitySet::default()
                });
            }
        }
    }
    for set in &mut sets {
        if let Some(t) = types.get(&set.entity_type) {
            set.keys.clone_from(&t.keys);
            set.properties.clone_from(&t.properties);
            set.navigation.clone_from(&t.navigation);
        }
    }
    Ok(ODataMetadata {
        version: root.attribute("Version").unwrap_or_default().to_owned(),
        entity_sets: sets,
    })
}
