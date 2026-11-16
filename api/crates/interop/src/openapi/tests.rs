use super::*;
use crate::detect::parse_document;

const PETSTORE: &str = r#"
openapi: 3.0.3
info:
  title: Petstore
  description: Pets.
servers:
  - url: https://{env}.pets.test/v1/
    variables:
      env: {default: api}
security:
  - bearer: []
paths:
  /pets:
    get:
      tags: [pets]
      summary: List pets
      parameters:
        - {name: limit, in: query, required: true, schema: {type: integer, example: 20}}
        - {name: sort, in: query, schema: {type: string, enum: [name, age]}}
        - {name: X-Trace, in: header, schema: {type: string}}
      responses:
        "200":
          description: A page of pets
          content:
            application/json:
              schema: {type: array, items: {$ref: '#/components/schemas/Pet'}}
    post:
      tags: [pets]
      operationId: createPet
      requestBody:
        $ref: '#/components/requestBodies/NewPet'
      responses:
        201: {description: Created}
  /pets/{petId}:
    parameters:
      - {name: petId, in: path, required: true, schema: {type: string}, example: p-1}
    delete:
      tags: [admin]
      security:
        - key: []
      responses: {"204": {description: Deleted}}
    put:
      security: []
      requestBody:
        content:
          multipart/form-data:
            schema:
              type: object
              properties:
                photo: {type: string, format: binary}
                caption: {type: string}
      responses: {"200": {description: ok}}
  /token:
    post:
      requestBody:
        content:
          application/x-www-form-urlencoded:
            schema:
              properties:
                grant_type: {type: string, default: password}
      responses: {"200": {description: ok}}
components:
  schemas:
    Pet:
      type: object
      properties:
        id: {type: integer, example: 7}
        name: {type: string}
  requestBodies:
    NewPet:
      content:
        application/json:
          examples:
            rex: {value: {name: Rex}}
  securitySchemes:
    bearer: {type: http, scheme: bearer}
    key: {type: apiKey, in: header, name: X-Key}
"#;

fn req<'a>(c: &'a Collection, name: &str) -> &'a SavedRequest {
    c.find_request_by(|r| r.name == name).expect(name)
}

trait Find {
    fn find_request_by(&self, f: impl Fn(&SavedRequest) -> bool) -> Option<&SavedRequest>;
}

impl Find for Collection {
    fn find_request_by(&self, f: impl Fn(&SavedRequest) -> bool) -> Option<&SavedRequest> {
        fn walk<'a>(
            items: &'a [Item],
            f: &dyn Fn(&SavedRequest) -> bool,
        ) -> Option<&'a SavedRequest> {
            items.iter().find_map(|i| match i {
                Item::Folder(fo) => walk(&fo.items, f),
                Item::Request(r) => f(r).then_some(r),
            })
        }
        walk(&self.items, &f)
    }
}

#[test]
fn builds_folders_requests_and_auth_from_a_yaml_document() {
    let c = import(&parse_document(PETSTORE).expect("yaml")).expect("import");
    assert_eq!(c.name, "Petstore");
    assert_eq!(c.variables[0].key, "baseUrl");
    assert_eq!(c.variables[0].value, "https://api.pets.test/v1");
    assert!(
        c.variables
            .iter()
            .any(|v| v.key == "petId" && v.value == "p-1")
    );
    assert!(matches!(&c.auth, Auth::Bearer { token } if token == "{{token}}"));
    let folders: Vec<&str> = c
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Folder(f) => Some(f.name.as_str()),
            Item::Request(_) => None,
        })
        .collect();
    assert_eq!(folders, ["pets", "admin"]);

    let list = req(&c, "List pets");
    assert_eq!(list.request.url, "{{baseUrl}}/pets?limit=20");
    let params: Vec<(&str, &str, bool)> = list
        .request
        .params
        .iter()
        .map(|p| (p.key.as_str(), p.value.as_str(), p.enabled))
        .collect();
    assert_eq!(params, [("limit", "20", true), ("sort", "name", false)]);
    assert_eq!(list.request.headers[0].key, "X-Trace");
    assert!(list.description.contains("- 200: A page of pets"));
    assert!(
        list.description.contains(r#"[{"id":7,"name":"string"}]"#),
        "{}",
        list.description
    );
    assert_eq!(list.request.auth, Auth::Inherit);

    let create = req(&c, "createPet");
    assert!(
        matches!(&create.request.body, RequestBody::Raw { text, .. } if text.contains("\"Rex\""))
    );
    assert!(create.description.contains("- 201: Created"));

    let delete = req(&c, "DELETE /pets/{petId}");
    assert_eq!(delete.request.url, "{{baseUrl}}/pets/{{petId}}");
    assert!(
        matches!(&delete.request.auth, Auth::ApiKey { key, location: ApiKeyLocation::Header, .. } if key == "X-Key")
    );

    let put = req(&c, "PUT /pets/{petId}");
    assert_eq!(put.request.auth, Auth::None);
    let RequestBody::FormData { fields } = &put.request.body else {
        panic!("multipart");
    };
    assert_eq!(fields[0].kind, FormFieldKind::File);
    assert_eq!(
        (fields[1].key.as_str(), fields[1].value.as_str()),
        ("caption", "string")
    );

    let token = req(&c, "POST /token");
    assert!(
        matches!(&token.request.body, RequestBody::UrlEncoded { fields } if fields[0].value == "password")
    );
}

#[test]
fn rejects_swagger_2_and_other_documents() {
    let swagger = serde_json::json!({"swagger": "2.0", "paths": {}});
    assert!(
        matches!(import(&swagger), Err(InteropError::Unsupported(m)) if m.contains("Swagger 2.0"))
    );
    assert!(import(&serde_json::json!({"a": 1})).is_err());
}
