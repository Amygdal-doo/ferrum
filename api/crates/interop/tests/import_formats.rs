//! Insomnia v4 / v5 exports and format detection (Postman, Insomnia,
//! OpenAPI, cURL).

use ferrumapi_domain::{
    ApiKeyLocation, Auth, FormFieldKind, HttpMethod, Item, OAuth2Grant, RawLanguage, RequestBody,
    RequestKind, SavedRequest,
};
use ferrumapi_interop::InteropError;
use ferrumapi_interop::detect::{Imported, import_collection, import_text};

const INSOMNIA_V4: &str = r#"{
  "_type": "export", "__export_format": 4, "__export_source": "insomnia.desktop.app:v2023.5.8",
  "resources": [
    {"_id": "wrk_1", "_type": "workspace", "parentId": null, "name": "Shop API"},
    {"_id": "env_1", "_type": "environment", "parentId": "wrk_1", "name": "Base Environment",
     "data": {"base_url": "https://shop.test", "page": 2, "nested": {"a": 1}}},
    {"_id": "env_2", "_type": "environment", "parentId": "env_1", "name": "Prod", "data": {}},
    {"_id": "fld_1", "_type": "request_group", "parentId": "wrk_1", "name": "Orders", "metaSortKey": 1,
     "authentication": {"type": "bearer", "token": "{{ _.token }}"}},
    {"_id": "req_2", "_type": "request", "parentId": "fld_1", "name": "Create order", "metaSortKey": 2,
     "method": "POST", "url": "{{ _.base_url }}/orders",
     "body": {"mimeType": "application/json", "text": "{\"sku\": \"{{ _.sku }}\"}"},
     "headers": [{"name": "X-Trace", "value": "1"}, {"name": "X-Off", "value": "0", "disabled": true}],
     "authentication": {}},
    {"_id": "req_1", "_type": "request", "parentId": "fld_1", "name": "List orders", "metaSortKey": 1,
     "method": "GET", "url": "{{ _.base_url }}/orders?status=open",
     "parameters": [{"name": "page", "value": "{{ _.page }}"}, {"name": "debug", "value": "1", "disabled": true}],
     "authentication": {"type": "apikey", "key": "k", "value": "v", "addTo": "queryParams"}},
    {"_id": "req_3", "_type": "request", "parentId": "wrk_1", "name": "Upload", "metaSortKey": 5,
     "method": "PUT", "url": "{{ _.base_url }}/files",
     "body": {"mimeType": "multipart/form-data", "params": [
        {"name": "file", "type": "file", "fileName": "/tmp/a.png"}, {"name": "note", "value": "hi"}]},
     "authentication": {"type": "oauth2", "grantType": "client_credentials",
        "accessTokenUrl": "https://id.test/token", "clientId": "cid", "clientSecret": "cs"}},
    {"_id": "req_4", "_type": "request", "parentId": "wrk_1", "name": "Search", "metaSortKey": 6,
     "method": "POST", "url": "{{ _.base_url }}/graphql",
     "body": {"mimeType": "application/graphql", "text": "{\"query\":\"{ me { id } }\",\"variables\":{\"a\":1}}"}},
    {"_id": "ws_1", "_type": "websocket_request", "parentId": "wrk_1", "name": "Live"}
  ]
}"#;

const INSOMNIA_V5: &str = r#"
type: collection.insomnia.rest/5.0
name: Billing
collection:
  - name: Invoices
    children:
      - url: "{{ _.base }}/invoices"
        name: List invoices
        method: GET
        authentication:
          type: iam
          accessKeyId: AK
          secretAccessKey: SK
          region: eu-west-1
          service: execute-api
  - url: "{{ _.base }}/health"
    name: Health
    method: GET
environments:
  name: Base Environment
  data:
    base: https://billing.test
"#;

fn requests(items: &[Item]) -> Vec<&SavedRequest> {
    items
        .iter()
        .flat_map(|i| match i {
            Item::Folder(f) => requests(&f.items),
            Item::Request(r) => vec![r],
        })
        .collect()
}

#[test]
fn insomnia_v4_export() {
    let (c, format) = import_collection(INSOMNIA_V4).expect("import");
    assert_eq!((c.name.as_str(), format), ("Shop API", "Insomnia"));
    let vars: Vec<(&str, &str)> = c
        .variables
        .iter()
        .map(|v| (v.key.as_str(), v.value.as_str()))
        .collect();
    assert_eq!(
        vars,
        [
            ("base_url", "https://shop.test"),
            ("page", "2"),
            ("nested", r#"{"a":1}"#)
        ]
    );
    assert!(c.description.contains("Sub-environments"));
    assert!(c.description.contains("websocket_request"));
    let Item::Folder(orders) = &c.items[0] else {
        panic!("folder first");
    };
    assert_eq!(orders.name, "Orders");
    assert!(matches!(&orders.auth, Auth::Bearer { token } if token == "{{token}}"));
    let all = requests(&c.items);
    let names: Vec<&str> = all.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["List orders", "Create order", "Upload", "Search"]);

    let list = &all[0].request;
    assert_eq!(list.url, "{{base_url}}/orders?status=open&page={{page}}");
    assert_eq!(list.params.len(), 3);
    assert!(!list.params[2].enabled);
    assert!(matches!(
        &list.auth,
        Auth::ApiKey {
            location: ApiKeyLocation::Query,
            ..
        }
    ));

    let create = &all[1].request;
    assert_eq!(create.method, HttpMethod::Post);
    assert_eq!(create.auth, Auth::Inherit, "empty authentication inherits");
    assert!(!create.headers[1].enabled);
    assert!(
        matches!(&create.body, RequestBody::Raw { language: RawLanguage::Json, text } if text == "{\"sku\": \"{{sku}}\"}")
    );

    let upload = &all[2].request;
    let RequestBody::FormData { fields } = &upload.body else {
        panic!("form");
    };
    assert_eq!(
        (fields[0].kind, fields[0].value.as_str()),
        (FormFieldKind::File, "/tmp/a.png")
    );
    assert!(
        matches!(&upload.auth, Auth::OAuth2(o) if o.grant == OAuth2Grant::ClientCredentials && o.client_secret == "cs")
    );

    let search = &all[3].request;
    assert_eq!(search.kind, RequestKind::Graphql);
    assert_eq!(search.specs.graphql.query, "{ me { id } }");
    assert!(search.specs.graphql.variables.contains("\"a\": 1"));
}

#[test]
fn insomnia_v5_yaml() {
    let (c, _) = import_collection(INSOMNIA_V5).expect("import");
    assert_eq!(c.name, "Billing");
    assert_eq!(c.variables[0].value, "https://billing.test");
    let all = requests(&c.items);
    assert_eq!(all[0].request.url, "{{base}}/invoices");
    assert!(
        matches!(&all[0].request.auth, Auth::AwsSigV4(a) if a.region == "eu-west-1" && a.secret_access_key == "SK")
    );
    assert_eq!(all[1].name, "Health");
}

#[test]
fn detects_each_format() {
    let curl = import_text("curl -X POST https://x.test/a -d 'k=v'").expect("curl");
    assert!(matches!(curl, Imported::Request(r) if r.method == HttpMethod::Post));
    let postman = r#"{"info": {"name": "P", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"}, "item": []}"#;
    assert!(
        matches!(import_text(postman), Ok(Imported::Collection(c, "Postman")) if c.name == "P")
    );
    let openapi = r#"{"openapi": "3.1.0", "info": {"title": "O"}, "paths": {"/a": {"get": {}}}}"#;
    assert!(
        matches!(import_text(openapi), Ok(Imported::Collection(c, "OpenAPI")) if c.name == "O")
    );
    assert!(matches!(
        import_text("just: yaml"),
        Err(InteropError::Unsupported(_))
    ));
    assert!(import_text("{not json or yaml: [").is_err());
    assert!(import_collection("curl https://x.test").is_err());
}
