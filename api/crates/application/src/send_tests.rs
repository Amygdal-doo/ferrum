use super::*;
use ferrumapi_domain::{Folder, Item, SavedRequest, Variable};

fn fixture() -> (Collection, RequestOrigin) {
    let mut c = Collection::new("C");
    c.auth = Auth::Bearer {
        token: "{{token}}".into(),
    };
    c.variables = vec![
        Variable::new("host", "coll.example"),
        Variable::new("only_coll", "1"),
    ];
    let mut f = Folder::new("F");
    let saved = SavedRequest::new(
        "R",
        HttpRequest::new(Default::default(), "https://{{host}}"),
    );
    let origin = RequestOrigin {
        collection: c.id,
        item: saved.id,
    };
    f.items.push(Item::Request(saved));
    c.items.push(Item::Folder(f));
    (c, origin)
}

#[test]
fn prepare_resolves_inherited_auth_and_variable_layers() {
    let (c, origin) = fixture();
    let mut globals = Environment::globals();
    globals.variables = vec![
        Variable::new("host", "global.example"),
        Variable::new("token", "g"),
    ];
    let mut env = Environment::new("dev");
    env.variables = vec![Variable::new("host", "env.example")];
    let tab = c.find_request(origin.item).expect("req").request.clone();

    let p = prepare(
        &tab,
        Some(origin),
        std::slice::from_ref(&c),
        &globals,
        Some(&env),
    );
    assert_eq!(
        p.request.auth,
        Auth::Bearer {
            token: "{{token}}".into()
        }
    );
    assert_eq!(p.variables.get("host"), Some("env.example"));
    assert_eq!(p.variables.get("only_coll"), Some("1"));
    assert_eq!(p.variables.get("token"), Some("g"));

    let loose = prepare(&tab, None, &[c], &globals, None);
    assert_eq!(loose.request.auth, Auth::None);
    assert_eq!(loose.variables.get("host"), Some("global.example"));
}

#[test]
fn unsaved_tab_auth_inherit_overrides_saved_auth() {
    let (mut c, origin) = fixture();
    c.find_request_mut(origin.item).expect("req").request.auth = Auth::None;
    let tab = HttpRequest::default(); // Inherit
    let p = prepare(&tab, Some(origin), &[c], &Environment::globals(), None);
    assert!(matches!(p.request.auth, Auth::Bearer { .. }));
}
