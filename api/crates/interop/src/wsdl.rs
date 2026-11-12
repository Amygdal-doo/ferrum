//! WSDL 1.1: list the SOAP operations of a service (with SOAPAction, SOAP
//! version and endpoint) and generate a sample request body for each.
//! Only the subset of XML Schema needed for a sample is understood:
//! elements, named / anonymous complex types with sequences and `ref`s.

use crate::InteropError;
use ferrumapi_domain::protocol::SoapVersion;
use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsdlOperation {
    pub name: String,
    pub action: String,
    pub version: SoapVersion,
    /// `soap:address` of the port using this binding, if any.
    pub endpoint: Option<String>,
    /// Sample contents of `<soap:Body>`.
    pub sample_body: String,
}

const MAX_DEPTH: usize = 4;

fn local<'a>(n: &Node<'a, '_>) -> &'a str {
    n.tag_name().name()
}

fn children<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children()
        .filter(move |c| c.is_element() && local(c) == name)
}

/// `tns:Foo` → `Foo`.
fn strip_prefix(qname: &str) -> &str {
    qname.rsplit(':').next().unwrap_or(qname)
}

struct Schema<'a, 'i> {
    elements: HashMap<&'a str, Node<'a, 'i>>,
    types: HashMap<&'a str, Node<'a, 'i>>,
    /// Target namespace and qualified children, per element name.
    namespaces: HashMap<&'a str, (&'a str, bool)>,
}

impl<'a, 'i> Schema<'a, 'i> {
    fn read(doc: &'a Document<'i>) -> Self {
        let mut elements = HashMap::new();
        let mut types = HashMap::new();
        let mut namespaces = HashMap::new();
        for schema in doc
            .descendants()
            .filter(|n| n.is_element() && local(n) == "schema")
        {
            let ns = schema.attribute("targetNamespace").unwrap_or_default();
            let qualified = schema.attribute("elementFormDefault") == Some("qualified");
            for child in schema.children().filter(Node::is_element) {
                let Some(name) = child.attribute("name") else {
                    continue;
                };
                match local(&child) {
                    "element" => {
                        elements.insert(name, child);
                        namespaces.insert(name, (ns, qualified));
                    }
                    "complexType" | "simpleType" => {
                        types.insert(name, child);
                    }
                    _ => {}
                }
            }
        }
        Self {
            elements,
            types,
            namespaces,
        }
    }

    /// Child elements of an element / complex type node, as sample XML lines.
    fn sample_children(
        &self,
        node: Node<'a, 'i>,
        prefix: &str,
        depth: usize,
        out: &mut Vec<String>,
    ) {
        if depth > MAX_DEPTH {
            return;
        }
        for el in node
            .descendants()
            .filter(|n| n.is_element() && local(n) == "element" && *n != node)
            .filter(|n| {
                // Direct members only: no other element between `node` and `n`.
                n.ancestors()
                    .skip(1)
                    .take_while(|a| *a != node)
                    .all(|a| !(a.is_element() && local(&a) == "element"))
            })
        {
            let (name, target) = match (el.attribute("name"), el.attribute("ref")) {
                (Some(name), _) => (name, Some(el)),
                (None, Some(r)) => {
                    let r = strip_prefix(r);
                    (r, self.elements.get(r).copied())
                }
                (None, None) => continue,
            };
            let pad = "  ".repeat(depth);
            let complex = target.and_then(|t| self.complex_of(t));
            match complex {
                Some(ct) if depth < MAX_DEPTH => {
                    out.push(format!("{pad}<{prefix}{name}>"));
                    self.sample_children(ct, prefix, depth + 1, out);
                    out.push(format!("{pad}</{prefix}{name}>"));
                }
                _ => out.push(format!("{pad}<{prefix}{name}>?</{prefix}{name}>")),
            }
        }
    }

    /// The complex type of an element: inline, or named via `type=`.
    fn complex_of(&self, el: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
        if let Some(ct) = children(el, "complexType").next() {
            return Some(ct);
        }
        let ty = self.types.get(strip_prefix(el.attribute("type")?))?;
        (local(ty) == "complexType").then_some(*ty)
    }

    fn element_sample(&self, name: &str) -> Option<String> {
        let el = *self.elements.get(name)?;
        let (ns, qualified) = self.namespaces.get(name).copied().unwrap_or_default();
        let child_prefix = if qualified { "tns:" } else { "" };
        let mut lines = vec![format!("<tns:{name} xmlns:tns=\"{ns}\">")];
        if let Some(ct) = self.complex_of(el) {
            self.sample_children(ct, child_prefix, 1, &mut lines);
        }
        lines.push(format!("</tns:{name}>"));
        Some(lines.join("\n"))
    }
}

/// Parses a WSDL 1.1 document. Operations appear once per SOAP binding
/// (a service with 1.1 and 1.2 bindings lists both), in document order.
pub fn parse(xml: &str) -> Result<Vec<WsdlOperation>, InteropError> {
    let doc = Document::parse(xml).map_err(|e| InteropError::Parse(format!("WSDL: {e}")))?;
    let root = doc.root_element();
    if local(&root) != "definitions" {
        return Err(InteropError::Unsupported(
            "not a WSDL 1.1 document (WSDL 2.0 is not supported)".into(),
        ));
    }
    let tns = root.attribute("targetNamespace").unwrap_or_default();
    let schema = Schema::read(&doc);
    // message name → parts (name, element or type)
    let messages: HashMap<&str, Vec<(&str, Option<&str>)>> = children(root, "message")
        .filter_map(|m| {
            let parts = children(m, "part")
                .map(|p| {
                    (
                        p.attribute("name").unwrap_or_default(),
                        p.attribute("element"),
                    )
                })
                .collect();
            Some((m.attribute("name")?, parts))
        })
        .collect();
    // portType operation → input message
    let inputs: HashMap<&str, &str> = children(root, "portType")
        .flat_map(|pt| children(pt, "operation"))
        .filter_map(|op| {
            let input = children(op, "input").next()?.attribute("message")?;
            Some((op.attribute("name")?, strip_prefix(input)))
        })
        .collect();
    // binding name → endpoint
    let endpoints: HashMap<&str, &str> = children(root, "service")
        .flat_map(|s| children(s, "port"))
        .filter_map(|port| {
            let address = children(port, "address").next()?.attribute("location")?;
            Some((strip_prefix(port.attribute("binding")?), address))
        })
        .collect();

    let mut out = Vec::new();
    for binding in children(root, "binding") {
        let Some(soap) = children(binding, "binding").next() else {
            continue; // HTTP bindings
        };
        let version = if soap
            .tag_name()
            .namespace()
            .is_some_and(|ns| ns.contains("soap12"))
        {
            SoapVersion::V12
        } else {
            SoapVersion::V11
        };
        let rpc = soap.attribute("style") == Some("rpc");
        let endpoint = binding
            .attribute("name")
            .and_then(|n| endpoints.get(n))
            .map(|s| (*s).to_owned());
        for op in children(binding, "operation") {
            let name = op.attribute("name").unwrap_or_default();
            let action = children(op, "operation")
                .next()
                .and_then(|o| o.attribute("soapAction"))
                .unwrap_or_default();
            let parts = inputs
                .get(name)
                .and_then(|m| messages.get(m))
                .cloned()
                .unwrap_or_default();
            let sample_body = sample_body(&schema, tns, name, rpc, &parts);
            out.push(WsdlOperation {
                name: name.to_owned(),
                action: action.to_owned(),
                version,
                endpoint: endpoint.clone(),
                sample_body,
            });
        }
    }
    if out.is_empty() {
        return Err(InteropError::Parse(
            "the WSDL has no SOAP bindings with operations".into(),
        ));
    }
    Ok(out)
}

fn sample_body(
    schema: &Schema<'_, '_>,
    tns: &str,
    operation: &str,
    rpc: bool,
    parts: &[(&str, Option<&str>)],
) -> String {
    if !rpc {
        let samples: Vec<String> = parts
            .iter()
            .filter_map(|(_, el)| schema.element_sample(strip_prefix((*el)?)))
            .collect();
        if !samples.is_empty() {
            return samples.join("\n");
        }
    }
    let mut lines = vec![format!("<tns:{operation} xmlns:tns=\"{tns}\">")];
    for (part, _) in parts {
        lines.push(format!("  <{part}>?</{part}>"));
    }
    lines.push(format!("</tns:{operation}>"));
    lines.join("\n")
}
