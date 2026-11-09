use crate::*;

fn sample() -> (Collection, ItemId, ItemId, ItemId) {
    let mut c = Collection::new("API");
    c.auth = Auth::Bearer {
        token: "root".into(),
    };
    let folder = Folder::new("Users");
    let fid = folder.id;
    c.insert(None, Item::Folder(folder));
    let r1 = SavedRequest::new("List", HttpRequest::new(HttpMethod::Get, "https://x/users"));
    let r1id = r1.id;
    assert!(c.insert(Some(fid), Item::Request(r1)));
    let r2 = SavedRequest::new("Ping", HttpRequest::default());
    let r2id = r2.id;
    c.insert(None, Item::Request(r2));
    (c, fid, r1id, r2id)
}

#[test]
fn find_insert_remove() {
    let (mut c, fid, r1, r2) = sample();
    assert_eq!(c.find_request(r1).map(|r| r.name.as_str()), Some("List"));
    assert!(c.find_request(fid).is_none());
    assert!(!c.insert(Some(r2), Item::Folder(Folder::new("x"))));
    assert!(c.remove(r1).is_some());
    assert!(c.find(r1).is_none());
    assert_eq!(c.requests().len(), 1);
}

#[test]
fn requests_under_folder_in_tree_order() {
    let (mut c, fid, r1, r2) = sample();
    let nested = Folder::new("Nested");
    let nid = nested.id;
    assert!(c.insert(Some(fid), Item::Folder(nested)));
    let deep = SavedRequest::new("Deep", HttpRequest::default());
    let deep_id = deep.id;
    assert!(c.insert(Some(nid), Item::Request(deep)));
    let ids = |list: Vec<&SavedRequest>| list.iter().map(|r| r.id).collect::<Vec<_>>();
    assert_eq!(ids(c.requests_under(None)), [r1, deep_id, r2]);
    assert_eq!(ids(c.requests_under(Some(fid))), [r1, deep_id]);
    assert_eq!(ids(c.requests_under(Some(nid))), [deep_id]);
    assert!(
        c.requests_under(Some(r2)).is_empty(),
        "a request is not a folder"
    );
    assert!(c.requests_under(Some(ItemId::new())).is_empty());
}

#[test]
fn rename_and_duplicate() {
    let (mut c, fid, _, _) = sample();
    assert!(c.rename(fid, "People"));
    let copy = c.duplicate_item(fid).expect("dup");
    assert_ne!(copy, fid);
    assert_eq!(c.items[1].name(), "People Copy");
    assert_eq!(c.requests().len(), 3);
    let ids: std::collections::HashSet<_> = c.requests().iter().map(|r| r.id).collect();
    assert_eq!(ids.len(), 3, "duplicated requests get fresh ids");
    let whole = c.duplicate();
    assert_ne!(whole.id, c.id);
    assert_eq!(whole.name, "API Copy");
}

#[test]
fn auth_inheritance() {
    let (mut c, fid, r1, r2) = sample();
    assert_eq!(
        c.effective_auth(r1),
        Auth::Bearer {
            token: "root".into()
        }
    );
    if let Some(Item::Folder(f)) = c.find_mut(fid) {
        f.auth = Auth::None;
    }
    assert_eq!(c.effective_auth(r1), Auth::None);
    assert_eq!(
        c.effective_auth(r2),
        Auth::Bearer {
            token: "root".into()
        }
    );
    c.find_request_mut(r2).expect("r2").request.auth = Auth::Bearer {
        token: "own".into(),
    };
    assert_eq!(
        c.effective_auth(r2),
        Auth::Bearer {
            token: "own".into()
        }
    );
    c.auth = Auth::Inherit;
    assert_eq!(c.effective_auth(ItemId::new()), Auth::None);
}

#[test]
fn collection_serde_round_trip() {
    let (c, ..) = sample();
    let json = serde_json::to_string(&c).expect("ser");
    let back: Collection = serde_json::from_str(&json).expect("de");
    assert_eq!(back, c);
    let empty: Collection = serde_json::from_str("{}").expect("defaults");
    assert!(empty.items.is_empty());
}
