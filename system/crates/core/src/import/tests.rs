use std::path::Path;

use super::*;
use crate::model::DeployTarget;

fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
    std::fs::write(path, text).expect("write");
}

fn edge(system: &System, from: &str, to: &str) -> Option<EdgeStyle> {
    system
        .edges
        .iter()
        .find(|e| e.from == from && e.to == to)
        .map(|e| e.style)
}

const COMPOSE: &str = r#"
services:
  web:
    build: ./web
    ports: ["3000:3000"]
    environment:
      API_URL: http://api:8080
  api:
    build: { context: ./api }
    ports:
      - "127.0.0.1:8080:8080/tcp"
    environment:
      - DATABASE_URL=postgres://app:secret@db:5432/app
      - QUEUE=amqp://rabbit
    depends_on:
      db: { condition: service_healthy }
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
    deploy: { replicas: 3 }
  db:
    image: postgres:16-alpine
    healthcheck:
      test: ["CMD-SHELL", "pg_isready"]
  rabbit:
    image: rabbitmq:3-management
  cache:
    image: bitnami/redis:7
    container_name: shop-cache
"#;

#[test]
fn a_compose_file_gives_services_their_kinds_and_connections() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(root, "docker-compose.yml", COMPOSE);
    write(
        root,
        "web/package.json",
        r#"{"dependencies": {"next": "15", "react": "19"}}"#,
    );
    write(
        root,
        "api/Cargo.toml",
        "[package]\nname = \"api\"\n[dependencies]\naxum = \"0.8\"\n",
    );
    // Not looked into.
    write(
        root,
        "node_modules/x/docker-compose.yml",
        "services: { ghost: { image: x } }",
    );

    let imported = from_project(root).expect("imported");
    assert_eq!(imported.sources, [Path::new("docker-compose.yml")]);
    let s = &imported.system;
    let kinds: Vec<_> = s
        .nodes
        .iter()
        .map(|n| {
            (
                n.id.as_str(),
                n.kind.as_str(),
                n.tech.as_deref(),
                n.layer.as_str(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("api", "service", Some("rust"), "services"),
            ("cache", "cache", Some("redis"), "data"),
            ("db", "sql-db", Some("postgresql"), "data"),
            ("rabbit", "queue", Some("rabbitmq"), "async"),
            ("web", "web-client", Some("nextjs"), "clients"),
        ]
    );
    let layers: Vec<_> = s.layers.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(layers, ["clients", "services", "async", "data"]);
    assert_eq!(edge(s, "web", "api"), Some(EdgeStyle::Request));
    assert_eq!(edge(s, "api", "db"), Some(EdgeStyle::Request));
    assert_eq!(edge(s, "api", "rabbit"), Some(EdgeStyle::Async));
    assert_eq!(s.edges.len(), 3);

    let api = s
        .node("api")
        .and_then(|n| n.deploy.clone())
        .expect("deploy");
    assert_eq!(api.target, DeployTarget::Container);
    assert_eq!((api.replicas, api.port), (Some(3), Some(8080)));
    assert_eq!(
        api.health.as_deref(),
        Some("curl -f http://localhost:8080/health")
    );
    let cache = s
        .node("cache")
        .and_then(|n| n.deploy.clone())
        .expect("deploy");
    assert_eq!(cache.selector.as_deref(), Some("shop-cache"));
    assert_eq!(cache.health, None);

    // The same files always give the same diagram.
    assert_eq!(from_project(root), Some(imported));
}

const K8S: &str = r#"
apiVersion: apps/v1
kind: Deployment
metadata: { name: orders, namespace: shop }
spec:
  replicas: 1
  template:
    metadata: { labels: { app: orders-app } }
    spec:
      containers:
        - name: orders
          image: ghcr.io/acme/orders:1.2
          ports: [{ containerPort: 9000 }]
          readinessProbe: { httpGet: { path: /ready, port: 9000 } }
          env:
            - { name: DB, value: "postgres://orders-db.shop.svc:5432/orders" }
---
apiVersion: v1
kind: Service
metadata: { name: orders-svc }
spec: { selector: { app: orders-app } }
---
apiVersion: apps/v1
kind: StatefulSet
metadata: { name: orders-db }
spec:
  replicas: 2
  template:
    metadata: { labels: { app: orders-db } }
    spec:
      containers: [{ name: pg, image: postgres:16 }]
---
apiVersion: batch/v1
kind: CronJob
metadata: { name: nightly-report }
spec:
  jobTemplate:
    spec:
      template:
        spec:
          containers: [{ name: report, image: python:3.12 }]
"#;

const INGRESS: &str = r#"
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata: { name: public }
spec:
  rules:
    - http:
        paths:
          - path: /
            backend: { service: { name: orders-svc, port: { number: 80 } } }
"#;

#[test]
fn kubernetes_manifests_give_pods_probes_and_ingress_routes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(root, "k8s/app.yaml", K8S);
    write(root, "k8s/ingress.yml", INGRESS);
    write(root, ".github/workflows/ci.yml", "on: push\njobs: {}\n");
    let imported = from_project(root).expect("imported");
    assert_eq!(imported.sources.len(), 2, "{:?}", imported.sources);
    let s = &imported.system;
    assert_eq!(
        s.cluster.as_ref().map(|c| c.namespace.as_str()),
        Some("shop")
    );
    let orders = s.node("orders").expect("orders");
    let deploy = orders.deploy.clone().expect("deploy");
    assert_eq!(deploy.target, DeployTarget::Pod);
    assert_eq!(deploy.selector.as_deref(), Some("orders-app"));
    assert_eq!(deploy.health.as_deref(), Some("readiness: GET /ready"));
    assert_eq!((deploy.replicas, deploy.port), (Some(1), Some(9000)));
    let db = s.node("orders-db").expect("db");
    assert_eq!((db.kind.as_str(), db.layer.as_str()), ("sql-db", "data"));
    assert_eq!(
        s.node("nightly-report").map(|n| n.kind.as_str()),
        Some("cron")
    );
    let public = s.node("public").expect("ingress");
    assert_eq!(
        (public.kind.as_str(), public.layer.as_str()),
        ("load-balancer", "edge")
    );
    assert!(edge(s, "public", "orders").is_some(), "{:?}", s.edges);
    assert!(edge(s, "orders", "orders-db").is_some(), "{:?}", s.edges);
}

#[test]
fn without_compose_or_kubernetes_the_package_manifests_are_used() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(root, "Cargo.toml", "[workspace]\nmembers = [\"server\"]\n");
    write(
        root,
        "server/Cargo.toml",
        "[package]\nname = \"server\"\n[dependencies]\nactix-web = \"4\"\nredis = \"0.27\"\n",
    );
    write(
        root,
        "worker/requirements.txt",
        "# jobs\nDjango>=5.0\npsycopg2-binary\npika==1.3\n",
    );
    write(root, "README.md", "# nothing here");
    let s = from_project(root).expect("imported").system;
    let ids: Vec<_> = s.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["postgres", "rabbitmq", "redis", "server", "worker"]);
    assert_eq!(
        s.node("worker").and_then(|n| n.tech.as_deref()),
        Some("django")
    );
    assert!(edge(&s, "server", "redis").is_some());
    assert_eq!(edge(&s, "worker", "rabbitmq"), Some(EdgeStyle::Async));

    let empty = tempfile::tempdir().expect("tempdir");
    write(empty.path(), "notes.txt", "hello");
    assert_eq!(from_project(empty.path()), None);
}

#[test]
fn merging_adds_only_what_is_missing() {
    let mut system = crate::demos::starter("Shop");
    let before = system.clone();
    let mut imported = system.clone();
    imported.nodes[0].name = "Renamed".into();
    imported.nodes.push(super::node("Search", "search", None));
    imported.edges.push(Edge {
        from: "api".into(),
        to: "search".into(),
        ..Edge::default()
    });
    merge(&mut system, &imported);
    assert_eq!(system.nodes[0], before.nodes[0], "kept as it was");
    assert_eq!(system.nodes.len(), before.nodes.len() + 1);
    assert_eq!(system.edges.len(), before.edges.len() + 1);
}

#[test]
fn addresses_name_services() {
    let names = ["db".to_string(), "api".into(), "orders".into()];
    let hits = |v: &str| -> Vec<&str> {
        mentioned(v, &names)
            .into_iter()
            .map(String::as_str)
            .collect()
    };
    assert_eq!(hits("postgres://u:p@db:5432/x"), ["db"]);
    assert_eq!(hits("http://orders.shop.svc/"), ["orders"]);
    assert_eq!(hits("api"), ["api"]);
    assert!(hits("rapid-db-tool").is_empty());
    assert!(hits("https://example.com/api").is_empty());
}
