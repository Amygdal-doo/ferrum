//! WSDL operations / sample envelopes and OData $metadata parsing.

use ferrumapi_domain::protocol::SoapVersion;
use ferrumapi_interop::{odata, wsdl};

const WSDL: &str = include_str!("fixtures/calculator.wsdl");
const METADATA: &str = include_str!("fixtures/odata_metadata.xml");

#[test]
fn wsdl_lists_operations_per_binding_with_actions_and_endpoints() {
    let ops = wsdl::parse(WSDL).expect("parse");
    let summary: Vec<_> = ops
        .iter()
        .map(|o| (o.name.as_str(), o.action.as_str(), o.version))
        .collect();
    assert_eq!(
        summary,
        [
            ("Add", "http://tempuri.org/Add", SoapVersion::V11),
            ("Lookup", "http://tempuri.org/Lookup", SoapVersion::V11),
            ("Add", "http://tempuri.org/Add", SoapVersion::V12),
        ]
    );
    assert_eq!(
        ops[0].endpoint.as_deref(),
        Some("http://www.dneonline.com/calculator.asmx")
    );
    assert_eq!(
        ops[0].sample_body,
        "<tns:Add xmlns:tns=\"http://tempuri.org/\">\n  <tns:intA>?</tns:intA>\n  <tns:intB>?</tns:intB>\n</tns:Add>"
    );
    // Named complex types are followed (nested `Options`).
    assert_eq!(
        ops[1].sample_body,
        "<tns:Lookup xmlns:tns=\"http://tempuri.org/\">\n  <tns:key>?</tns:key>\n  <tns:options>\n    \
         <tns:verbose>?</tns:verbose>\n  </tns:options>\n</tns:Lookup>"
    );
}

#[test]
fn wsdl_errors_are_explained() {
    assert!(wsdl::parse("<not-closed").is_err());
    assert!(matches!(
        wsdl::parse("<description xmlns=\"http://www.w3.org/ns/wsdl\"/>"),
        Err(ferrumapi_interop::InteropError::Unsupported(_))
    ));
    assert!(wsdl::parse("<definitions/>").is_err());
}

#[test]
fn odata_metadata_lists_entity_sets_with_their_types() {
    let m = odata::parse_metadata(METADATA).expect("parse");
    assert_eq!(m.version, "4.0");
    let names: Vec<_> = m.entity_sets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["People", "Airports"]);
    let people = &m.entity_sets[0];
    assert_eq!(people.entity_type, "Trippin.Person");
    assert_eq!(people.keys, ["UserName"]);
    assert_eq!(
        people.properties,
        [
            ("UserName".to_owned(), "Edm.String".to_owned()),
            ("Age".to_owned(), "Edm.Int32".to_owned())
        ]
    );
    assert_eq!(people.navigation, ["Friends"]);
    assert!(odata::parse_metadata("{\"value\":[]}").is_err());
    assert!(odata::parse_metadata("<feed/>").is_err());
}
