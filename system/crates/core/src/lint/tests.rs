use super::*;
use crate::demos::{edge, layer, node};
use crate::model::{Api, Deploy, Flow, Step};

fn deploy(target: DeployTarget, replicas: u32, health: Option<&str>, image: &str) -> Deploy {
    Deploy {
        target,
        image: Some(image.into()),
        replicas: Some(replicas),
        health: health.map(Into::into),
        ..Deploy::default()
    }
}

/// A shop with one problem of each kind.
fn shop() -> System {
    let mut web = node("web", "Web", "web-client", Some("react"), "clients", "");
    web.deploy = Some(Deploy {
        target: DeployTarget::Device,
        ..Deploy::default()
    });
    let mut api = node("api", "API", "service", Some("rust"), "services", "");
    api.deploy = Some(deploy(DeployTarget::Pod, 1, None, "ghcr.io/acme/api:1.2"));
    let mut worker = node("worker", "Worker", "worker", None, "services", "");
    worker.deploy = Some(deploy(
        DeployTarget::Container,
        1,
        Some("GET /health"),
        "acme/worker",
    ));
    let mut db = node("db", "DB", "sql-db", Some("postgresql"), "data", "");
    db.deploy = Some(deploy(
        DeployTarget::Pod,
        3,
        Some("pg_isready"),
        "postgres:16",
    ));
    let lonely = node("lonely", "Lonely", "service", None, "services", "");
    System {
        layers: vec![
            layer("clients", "Clients", 0),
            layer("services", "Services", 0),
            layer("data", "Data", 0),
        ],
        nodes: vec![web, api, worker, db, lonely],
        edges: vec![
            edge("web", "api", EdgeStyle::Request, Some("HTTPS")),
            edge("web", "db", EdgeStyle::Request, None),
            edge("api", "worker", EdgeStyle::Async, None),
            edge("api", "ghost", EdgeStyle::Rpc, None),
        ],
        apis: vec![Api {
            method: "GET".into(),
            path: "/x".into(),
            service: "gone".into(),
            summary: String::new(),
        }],
        flows: vec![Flow {
            name: "Checkout".into(),
            summary: String::new(),
            steps: vec![Step {
                from: "web".into(),
                to: "worker".into(),
                text: String::new(),
            }],
        }],
        ..System::default()
    }
}

fn found(system: &System) -> Vec<(Rule, Option<String>)> {
    check(system)
        .into_iter()
        .map(|p| (p.rule, p.node))
        .collect()
}

fn owned(expected: &[(Rule, Option<&str>)]) -> Vec<(Rule, Option<String>)> {
    expected
        .iter()
        .map(|(r, n)| (*r, n.map(str::to_owned)))
        .collect()
}

#[test]
fn finds_each_kind_of_problem_on_its_component() {
    let s = shop();
    assert_eq!(
        found(&s),
        owned(&[
            (Rule::PublicDatabase, Some("db")),
            (Rule::NoAuth, Some("api")),
            (Rule::SingleReplica, Some("api")),
            (Rule::SingleReplica, Some("worker")),
            (Rule::NoHealthCheck, Some("api")),
            (Rule::Dangling, Some("api")),
            (Rule::Dangling, None),
            (Rule::UnpinnedImage, Some("worker")),
            (Rule::Isolated, Some("lonely")),
        ])
    );
    assert!(
        check(&s)[0]
            .message
            .contains("DB is reached directly from Web")
    );
}

#[test]
fn fixing_the_design_clears_the_problems() {
    let mut s = shop();
    s.edges.retain(|e| e.to != "db" && e.to != "ghost");
    s.edges[0].auth = Some("JWT".into());
    s.apis.clear();
    s.nodes.retain(|n| n.id != "lonely");
    for n in &mut s.nodes {
        if let Some(d) = n
            .deploy
            .as_mut()
            .filter(|d| d.target != DeployTarget::Device)
        {
            d.replicas = Some(2);
            d.health = Some("GET /health".into());
            d.image = d
                .image
                .take()
                .map(|i| format!("{}:1.0", i.split(':').next().unwrap_or(&i)));
        }
    }
    s.edges.push(edge("api", "db", EdgeStyle::Request, None));
    assert_eq!(check(&s), []);
    // An auth word in the label counts too.
    s.edges[0].auth = None;
    s.edges[0].label = Some("HTTPS + OAuth".into());
    assert_eq!(check(&s), []);
}

#[test]
fn a_gateway_that_calls_identity_authenticates_and_demos_have_no_warnings_of_unknown_references() {
    for system in crate::demos::all() {
        let problems = check(&system);
        assert!(
            !problems.iter().any(|p| p.rule == Rule::Dangling),
            "{}: {problems:?}",
            system.name
        );
        assert!(
            !problems
                .iter()
                .any(|p| p.rule == Rule::NoAuth && p.node.as_deref() == Some("payments")),
            "{}: {problems:?}",
            system.name
        );
    }
    assert!(image_unpinned("nginx") && image_unpinned("a/b:latest"));
    assert!(!image_unpinned("localhost:5000/a:1") && !image_unpinned("a@sha256:x"));
}
