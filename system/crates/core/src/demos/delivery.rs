//! Live delivery tracking: customer, courier and restaurant apps, GPS
//! pings every few seconds, dispatch, and the courier's dot moving on the
//! customer's map over WebSockets.

use super::*;

pub fn delivery() -> System {
    use EdgeStyle::*;
    System {
        name: "Swift Courier".into(),
        tagline: "Food delivery with live courier tracking on every phone".into(),
        overview: "Three apps share one backend: customers order, restaurants accept, couriers \
                   deliver. Couriers' phones send their position every 4 seconds; the location \
                   service keeps only the latest point per courier in Redis (geo index) and \
                   streams the history to Kafka. Dispatch picks the nearest free courier, and \
                   the realtime gateway pushes positions to the customer's map over a WebSocket."
            .into(),
        provider: Provider::Gcp,
        layers: standard_layers(),
        nodes: vec![
            node(
                "customer",
                "Customer app",
                "mobile-client",
                Some("react-native"),
                "clients",
                "Order, pay, follow the courier on a map.",
            )
            .device()
            .slot(0.),
            node(
                "courier",
                "Courier app",
                "mobile-client",
                Some("android"),
                "clients",
                "Background GPS, accept jobs, proof of delivery.",
            )
            .device()
            .slot(1.),
            node(
                "courier-ios",
                "Courier app (iOS)",
                "mobile-client",
                Some("ios"),
                "clients",
                "The same for iPhone couriers.",
            )
            .device()
            .slot(2.),
            node(
                "restaurant",
                "Restaurant tablet",
                "web-client",
                Some("vue"),
                "clients",
                "Incoming orders, prep time, ready button.",
            )
            .device()
            .slot(3.),
            node(
                "lb",
                "Load balancer",
                "load-balancer",
                None,
                "edge",
                "HTTPS and WebSocket upgrade, sticky by connection.",
            )
            .managed()
            .slot(0.),
            node(
                "gateway",
                "API gateway",
                "api-gateway",
                None,
                "edge",
                "JWT check, per-app rate limits.",
            )
            .managed()
            .slot(1.),
            node(
                "realtime",
                "Realtime gateway",
                "realtime",
                Some("elixir"),
                "edge",
                "Millions of WebSockets; one channel per order.",
            )
            .pods(6)
            .slot(2.),
            node(
                "orders",
                "Orders",
                "service",
                Some("kotlin"),
                "services",
                "Cart, checkout, order state machine.",
            )
            .pods(4)
            .slot(0.),
            node(
                "dispatch",
                "Dispatch",
                "service",
                Some("go"),
                "services",
                "Matches orders to couriers every second.",
            )
            .pods(3)
            .items(&["City lease", "Courier ranking"])
            .slot(1.),
            node(
                "location",
                "Location",
                "service",
                Some("rust"),
                "services",
                "Ingests 50k GPS pings per second.",
            )
            .pods(6)
            .items(&["Ping validator", "Geo writer"])
            .slot(2.),
            node(
                "eta",
                "ETA model",
                "ml",
                Some("python"),
                "services",
                "Predicts pickup and drop-off times.",
            )
            .pods(2)
            .slot(3.),
            node(
                "payments",
                "Payments",
                "external",
                Some("stripe"),
                "services",
                "Card payments and payouts.",
            )
            .slot(4.),
            node(
                "pings",
                "Location stream",
                "stream",
                Some("kafka"),
                "async",
                "courier.location, partitioned by courier id.",
            )
            .managed()
            .slot(1.),
            node(
                "order-events",
                "Order events",
                "pubsub",
                Some("nats"),
                "async",
                "order.* fan-out to realtime and push.",
            )
            .managed()
            .slot(2.),
            node(
                "push",
                "Push notifications",
                "email",
                None,
                "async",
                "\"Your courier is 2 minutes away\".",
            )
            .managed()
            .slot(3.),
            node(
                "orders-db",
                "Orders DB",
                "sql-db",
                Some("postgresql"),
                "data",
                "",
            )
            .db("orders")
            .managed()
            .slot(0.),
            node(
                "geo",
                "Live positions",
                "cache",
                Some("redis"),
                "data",
                "GEOADD per courier; GEOSEARCH for dispatch.",
            )
            .managed()
            .slot(1.),
            node(
                "tracks",
                "Track history",
                "time-series",
                Some("timescaledb"),
                "data",
                "",
            )
            .db("tracks")
            .managed()
            .slot(2.),
            node(
                "analytics",
                "Analytics",
                "warehouse",
                None,
                "data",
                "Delivery times, heat maps, courier pay.",
            )
            .managed()
            .slot(3.),
            node(
                "gke",
                "Kubernetes",
                "cluster",
                Some("kubernetes"),
                "platform",
                "Autoscaling on pings per second.",
            )
            .managed()
            .slot(0.),
            node(
                "ci",
                "CI/CD",
                "ci",
                Some("argocd"),
                "platform",
                "GitOps; canary for the realtime gateway.",
            )
            .managed()
            .slot(1.),
        ],
        edges: vec![
            edge("customer", "lb", Request, Some("HTTPS")),
            edge("restaurant", "lb", Request, None),
            edge("courier", "lb", Request, Some("pings")),
            edge("courier-ios", "lb", Request, Some("pings")),
            edge("lb", "gateway", Request, None),
            edge("lb", "realtime", Socket, Some("WebSocket")),
            edge("gateway", "orders", Rpc, None),
            edge("gateway", "location", Rpc, Some("POST /pings")),
            edge("orders", "payments", Request, Some("PaymentIntent")),
            edge("orders", "orders-db", Request, Some("SQL")),
            edge("orders", "order-events", Async, Some("order.*")),
            edge("location", "geo", Request, Some("GEOADD")),
            edge("location", "pings", Async, Some("publish")),
            edge("pings", "tracks", Async, Some("sink")),
            edge("pings", "realtime", Async, Some("positions")),
            edge("dispatch", "geo", Request, Some("GEOSEARCH")),
            edge("dispatch", "eta", Rpc, None),
            edge("dispatch", "orders", Rpc, Some("assign")),
            edge("order-events", "realtime", Async, None),
            edge("order-events", "push", Async, None),
            edge("realtime", "customer", Socket, Some("live map")),
            edge("orders-db", "analytics", Cdc, Some("CDC")),
        ],
        concepts: strings(&[
            "Geospatial index",
            "WebSocket fan-out",
            "Partitioning by key",
            "State machines",
            "Back-pressure on high-rate ingest",
            "Time-series retention",
        ]),
        functional: strings(&[
            "Customers order, pay and follow the courier live on a map",
            "Restaurants accept orders and mark them ready",
            "Couriers get the nearest job and navigate to it",
            "Everyone gets a push when the order changes state",
        ]),
        non_functional: strings(&[
            "Courier position on the customer's map at most 5 s old",
            "50k location pings per second at peak, 10× on New Year's Eve",
            "Dispatch decision within 2 s of an order being ready",
            "Losing a few pings is fine; losing an order is not",
        ]),
        apis: vec![
            api(
                "POST",
                "/v1/orders",
                "orders",
                "Place an order from a cart.",
            ),
            api(
                "POST",
                "/v1/orders/{id}/ready",
                "orders",
                "Restaurant marks the order ready.",
            ),
            api(
                "POST",
                "/v1/couriers/me/pings",
                "location",
                "Batch of GPS points (lat, lng, accuracy, t).",
            ),
            api(
                "POST",
                "/v1/jobs/{id}/accept",
                "dispatch",
                "Courier accepts a delivery.",
            ),
        ],
        sockets: vec![
            socket(
                "wss://rt.swift.app/orders/{id}",
                "realtime",
                "server → client",
                "Order state and courier position for the customer's map.",
            ),
            socket(
                "wss://rt.swift.app/couriers/me",
                "realtime",
                "both",
                "Job offers to the courier; acknowledgements back.",
            ),
        ],
        databases: vec![
            Database {
                id: "orders".into(),
                name: "Orders DB".into(),
                tables: vec![
                    table(
                        "orders",
                        "State machine: placed → accepted → ready → picked_up → delivered",
                        &[
                            ("id", "uuid", "pk"),
                            ("customer_id", "uuid", ""),
                            ("restaurant_id", "uuid", ""),
                            ("courier_id", "uuid", "null"),
                            ("state", "text", ""),
                            ("total", "numeric(10,2)", ""),
                            ("placed_at", "timestamptz", ""),
                        ],
                    ),
                    table(
                        "order_items",
                        "",
                        &[
                            ("id", "bigserial", "pk"),
                            ("order_id", "uuid", "fk:orders.id"),
                            ("item", "text", ""),
                            ("qty", "int", ""),
                        ],
                    ),
                    table(
                        "couriers",
                        "",
                        &[
                            ("id", "uuid", "pk"),
                            ("name", "text", ""),
                            ("vehicle", "text", ""),
                            ("available", "boolean", ""),
                        ],
                    ),
                ],
            },
            Database {
                id: "tracks".into(),
                name: "Track history".into(),
                tables: vec![table(
                    "pings",
                    "Hypertable, 30 days kept, compressed after 1 day",
                    &[
                        ("courier_id", "uuid", "pk"),
                        ("t", "timestamptz", "pk"),
                        ("lat", "double", ""),
                        ("lng", "double", ""),
                        ("accuracy_m", "real", "null"),
                    ],
                )],
            },
        ],
        flows: vec![
            flow(
                "Courier moves, map updates",
                "Every 4 seconds per courier.",
                &[
                    ("courier", "lb", "The app sends a batch of GPS points."),
                    (
                        "gateway",
                        "location",
                        "Location validates and drops impossible jumps.",
                    ),
                    (
                        "location",
                        "geo",
                        "The latest point replaces the old one in Redis.",
                    ),
                    (
                        "location",
                        "pings",
                        "The point is published, keyed by courier.",
                    ),
                    (
                        "pings",
                        "realtime",
                        "The realtime gateway forwards it to that order's channel.",
                    ),
                    ("realtime", "customer", "The customer's map moves the dot."),
                ],
            ),
            flow(
                "Order to dispatch",
                "From checkout to a courier on the way.",
                &[
                    ("customer", "lb", "Checkout."),
                    ("orders", "payments", "Card authorised."),
                    (
                        "orders",
                        "order-events",
                        "order.placed goes to the restaurant tablet via realtime.",
                    ),
                    ("dispatch", "geo", "Nearest free couriers within 3 km."),
                    ("dispatch", "eta", "ETA ranks them."),
                    (
                        "dispatch",
                        "orders",
                        "The best courier is assigned; they get the offer.",
                    ),
                ],
            ),
        ],
        reliability: vec![
            note(
                "Ping storms",
                "Location is stateless and scales on CPU; Kafka absorbs bursts; old pings are dropped, not queued.",
            ),
            note(
                "WebSocket reconnects",
                "Clients resume with the last event id; the gateway replays from NATS JetStream.",
            ),
            note(
                "Dispatch is single-writer per city",
                "A lease per city avoids two dispatchers assigning the same courier.",
            ),
            note(
                "Degraded mode",
                "If realtime is down the app polls GET /orders/{id} every 10 s.",
            ),
        ],
        cluster: Some(Cluster {
            context: None,
            namespace: "swift".into(),
        }),
        custom_techs: Vec::new(),
    }
}
