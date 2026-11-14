//! Core banking on microservices: accounts, a double-entry ledger, payments
//! with fraud scoring, and notifications, joined by events.

use super::*;

pub fn banking() -> System {
    use EdgeStyle::*;
    System {
        name: "Nova Bank".into(),
        tagline: "Core banking on microservices: accounts, ledger, payments, fraud".into(),
        overview: "Customers use the mobile and web apps to open accounts, see balances and \
                   send money. Every movement of money is a pair of entries in an append-only \
                   ledger, the single source of truth for balances. Payments go through a saga: \
                   fraud scoring, a hold on the sender, settlement, then the ledger, each step \
                   an event on Kafka so a failure can be compensated instead of half-applied."
            .into(),
        provider: Provider::Aws,
        layers: standard_layers(),
        nodes: vec![
            node(
                "mobile",
                "Mobile banking",
                "mobile-client",
                Some("flutter"),
                "clients",
                "iOS and Android app with biometric sign-in.",
            )
            .device()
            .slot(0.),
            node(
                "web",
                "Online banking",
                "web-client",
                Some("angular"),
                "clients",
                "Browser app for customers and back-office staff.",
            )
            .device()
            .slot(1.),
            node(
                "partners",
                "Open banking partners",
                "external",
                None,
                "clients",
                "Third parties using the PSD2 APIs.",
            )
            .slot(2.),
            node(
                "waf",
                "WAF",
                "waf",
                None,
                "edge",
                "Blocks bots and injection before anything reaches the APIs.",
            )
            .managed()
            .slot(0.),
            node(
                "gateway",
                "API gateway",
                "api-gateway",
                Some("graphql"),
                "edge",
                "One entry point: auth, rate limits, request ids.",
            )
            .managed()
            .slot(1.),
            node(
                "auth",
                "Identity",
                "auth",
                Some("keycloak"),
                "edge",
                "OAuth 2.1 / OIDC, step-up for payments, device binding.",
            )
            .pods(2)
            .slot(2.),
            node(
                "accounts",
                "Accounts",
                "service",
                Some("java"),
                "services",
                "Customers, accounts, limits and KYC status.",
            )
            .pods(3)
            .slot(0.),
            node(
                "payments",
                "Payments",
                "service",
                Some("go"),
                "services",
                "Runs the payment saga and its compensations.",
            )
            .pods(4)
            .items(&["Saga orchestrator", "Compensations", "Idempotency keys"])
            .slot(1.),
            node(
                "ledger",
                "Ledger",
                "service",
                Some("rust"),
                "services",
                "Double-entry, append-only; the only writer of balances.",
            )
            .pods(3)
            .items(&["Journal writer", "Balance projection"])
            .slot(2.),
            node(
                "fraud",
                "Fraud scoring",
                "service",
                Some("python"),
                "services",
                "Scores each payment in under 50 ms.",
            )
            .pods(2)
            .slot(3.),
            node(
                "notify",
                "Notifications",
                "worker",
                Some("nodejs"),
                "services",
                "Push, e-mail and SMS from events.",
            )
            .pods(2)
            .slot(4.),
            node(
                "events",
                "Event bus",
                "stream",
                Some("kafka"),
                "async",
                "payment.*, ledger.*, account.* topics; 7 days kept.",
            )
            .managed()
            .slot(1.),
            node(
                "outbox",
                "Outbox relay",
                "worker",
                Some("go"),
                "async",
                "Publishes each service's outbox rows exactly once.",
            )
            .pods(2)
            .slot(2.),
            node(
                "accounts-db",
                "Accounts DB",
                "sql-db",
                Some("postgresql"),
                "data",
                "",
            )
            .db("accounts")
            .managed()
            .slot(0.),
            node(
                "payments-db",
                "Payments DB",
                "sql-db",
                Some("postgresql"),
                "data",
                "",
            )
            .db("payments")
            .managed()
            .slot(1.),
            node(
                "ledger-db",
                "Ledger DB",
                "sql-db",
                Some("postgresql"),
                "data",
                "",
            )
            .db("ledger")
            .managed()
            .slot(2.),
            node(
                "cache",
                "Session & limits cache",
                "cache",
                Some("redis"),
                "data",
                "Sessions, idempotency keys, daily limits.",
            )
            .managed()
            .slot(3.),
            node(
                "warehouse",
                "Reporting warehouse",
                "warehouse",
                Some("clickhouse"),
                "data",
                "Regulatory reports and analytics, fed by CDC.",
            )
            .managed()
            .slot(4.),
            node(
                "cluster",
                "Kubernetes",
                "cluster",
                Some("kubernetes"),
                "platform",
                "Three zones, PodDisruptionBudgets, network policies.",
            )
            .managed()
            .slot(0.),
            node(
                "secrets",
                "Secrets & keys",
                "secrets",
                Some("vault"),
                "platform",
                "DB credentials rotated hourly; signing keys in an HSM.",
            )
            .managed()
            .slot(1.),
            node(
                "observability",
                "Observability",
                "monitoring",
                Some("grafana"),
                "platform",
                "SLOs per API, traces across the saga.",
            )
            .managed()
            .slot(2.),
        ],
        edges: vec![
            edge("mobile", "waf", Request, Some("HTTPS")),
            edge("web", "waf", Request, Some("HTTPS")),
            edge("partners", "waf", Request, Some("mTLS")),
            edge("waf", "gateway", Request, None),
            edge("gateway", "auth", Rpc, Some("token check")),
            edge("gateway", "accounts", Rpc, None),
            edge("gateway", "payments", Rpc, None),
            edge("payments", "fraud", Rpc, Some("score")),
            edge("payments", "ledger", Rpc, Some("post entries")),
            edge("accounts", "accounts-db", Request, Some("SQL")),
            edge("payments", "payments-db", Request, Some("SQL + outbox")),
            edge("ledger", "ledger-db", Request, Some("SQL")),
            edge("payments", "cache", Request, Some("idempotency")),
            edge("outbox", "events", Async, Some("publish")),
            edge("payments-db", "outbox", Cdc, Some("outbox rows")),
            edge("events", "notify", Async, Some("payment.settled")),
            edge("events", "ledger", Async, Some("payment.authorized")),
            edge("ledger-db", "warehouse", Cdc, Some("CDC")),
            edge("notify", "mobile", Socket, Some("push")),
        ],
        concepts: strings(&[
            "Double-entry ledger",
            "Saga with compensations",
            "Transactional outbox",
            "Idempotency keys",
            "Change data capture",
            "Strong consistency for balances",
        ]),
        functional: strings(&[
            "Open an account after KYC checks",
            "See balances and transactions in real time",
            "Send domestic and SEPA payments with a one-time confirmation",
            "Freeze a card or an account instantly",
            "Give licensed partners access through open banking APIs",
        ]),
        non_functional: strings(&[
            "Never lose or double-apply money: every payment exactly once",
            "Balances strongly consistent; notifications and reports may lag",
            "99.95% availability for payments, p99 under 300 ms",
            "Audit trail kept 10 years, tamper-evident",
            "Data stays in the EU",
        ]),
        apis: vec![
            api(
                "POST",
                "/v1/accounts",
                "accounts",
                "Open an account (after KYC).",
            ),
            api(
                "GET",
                "/v1/accounts/{id}/balance",
                "ledger",
                "Current and available balance.",
            ),
            api(
                "GET",
                "/v1/accounts/{id}/transactions",
                "ledger",
                "Paged ledger entries.",
            ),
            api(
                "POST",
                "/v1/payments",
                "payments",
                "Start a payment; requires Idempotency-Key.",
            ),
            api(
                "GET",
                "/v1/payments/{id}",
                "payments",
                "Payment state in the saga.",
            ),
            api(
                "POST",
                "/v1/cards/{id}/freeze",
                "accounts",
                "Freeze a card immediately.",
            ),
        ],
        sockets: vec![socket(
            "wss://api.novabank.eu/v1/events",
            "notify",
            "server → client",
            "Payment and balance updates while the app is open; push when it is not.",
        )],
        databases: vec![
            Database {
                id: "accounts".into(),
                name: "Accounts DB".into(),
                tables: vec![
                    table(
                        "customers",
                        "People and companies",
                        &[
                            ("id", "uuid", "pk"),
                            ("legal_name", "text", ""),
                            ("kyc_status", "text", ""),
                            ("created_at", "timestamptz", ""),
                        ],
                    ),
                    table(
                        "accounts",
                        "IBAN accounts",
                        &[
                            ("id", "uuid", "pk"),
                            ("customer_id", "uuid", "fk:customers.id"),
                            ("iban", "varchar(34)", ""),
                            ("currency", "char(3)", ""),
                            ("status", "text", ""),
                            ("daily_limit", "numeric(18,2)", ""),
                        ],
                    ),
                    table(
                        "cards",
                        "Debit cards",
                        &[
                            ("id", "uuid", "pk"),
                            ("account_id", "uuid", "fk:accounts.id"),
                            ("last4", "char(4)", ""),
                            ("frozen", "boolean", ""),
                        ],
                    ),
                ],
            },
            Database {
                id: "payments".into(),
                name: "Payments DB".into(),
                tables: vec![
                    table(
                        "payments",
                        "One row per payment saga",
                        &[
                            ("id", "uuid", "pk"),
                            ("idempotency_key", "text", ""),
                            ("from_account", "uuid", ""),
                            ("to_iban", "varchar(34)", ""),
                            ("amount", "numeric(18,2)", ""),
                            ("state", "text", ""),
                            ("fraud_score", "real", "null"),
                        ],
                    ),
                    table(
                        "outbox",
                        "Events to publish, written in the same transaction",
                        &[
                            ("id", "bigserial", "pk"),
                            ("aggregate_id", "uuid", ""),
                            ("topic", "text", ""),
                            ("payload", "jsonb", ""),
                            ("published_at", "timestamptz", "null"),
                        ],
                    ),
                ],
            },
            Database {
                id: "ledger".into(),
                name: "Ledger DB".into(),
                tables: vec![
                    table(
                        "journal",
                        "One business event",
                        &[
                            ("id", "uuid", "pk"),
                            ("payment_id", "uuid", "null"),
                            ("booked_at", "timestamptz", ""),
                        ],
                    ),
                    table(
                        "entries",
                        "Debits and credits; they sum to zero per journal",
                        &[
                            ("id", "bigserial", "pk"),
                            ("journal_id", "uuid", "fk:journal.id"),
                            ("account_id", "uuid", ""),
                            ("amount", "numeric(18,2)", ""),
                            ("side", "char(1)", ""),
                        ],
                    ),
                    table(
                        "balances",
                        "Running balance, updated with the entries",
                        &[
                            ("account_id", "uuid", "pk"),
                            ("booked", "numeric(18,2)", ""),
                            ("available", "numeric(18,2)", ""),
                            ("version", "bigint", ""),
                        ],
                    ),
                ],
            },
        ],
        flows: vec![
            flow(
                "Send a payment",
                "From tapping Send to both balances changing.",
                &[
                    (
                        "mobile",
                        "waf",
                        "The app sends POST /v1/payments with an Idempotency-Key.",
                    ),
                    (
                        "gateway",
                        "auth",
                        "The gateway checks the token and asks for step-up (Face ID).",
                    ),
                    (
                        "gateway",
                        "payments",
                        "Payments stores the request and the key; a retry returns the same payment.",
                    ),
                    (
                        "payments",
                        "fraud",
                        "Fraud scores it; above the threshold the payment waits for review.",
                    ),
                    (
                        "payments",
                        "ledger",
                        "The ledger posts a hold: debit sender, credit the clearing account.",
                    ),
                    (
                        "payments-db",
                        "outbox",
                        "payment.settled is written to the outbox in the same transaction.",
                    ),
                    ("outbox", "events", "The relay publishes it once."),
                    ("events", "notify", "Notifications tells both customers."),
                ],
            ),
            flow(
                "Monthly statement",
                "Reports never touch the live databases.",
                &[
                    (
                        "ledger-db",
                        "warehouse",
                        "CDC streams every entry into ClickHouse.",
                    ),
                    (
                        "warehouse",
                        "notify",
                        "A job renders statements and Notifications e-mails them.",
                    ),
                ],
            ),
        ],
        reliability: vec![
            note(
                "Exactly once",
                "Idempotency keys at the API, the outbox for events and unique constraints in the ledger make retries safe.",
            ),
            note(
                "Failure of a saga step",
                "Each step has a compensation (release hold, refund); the saga state lives in payments.payments.",
            ),
            note(
                "Zones",
                "Three replicas per service across three zones; databases Multi-AZ with synchronous standby.",
            ),
            note(
                "Back-pressure",
                "Kafka absorbs bursts; consumers scale on lag. Payments rejects with 429 before queues overflow.",
            ),
            note(
                "Disaster recovery",
                "RPO 0 for the ledger (synchronous replica), RTO 30 minutes to a second region.",
            ),
        ],
        cluster: Some(Cluster {
            context: None,
            namespace: "nova-bank".into(),
        }),
        custom_techs: Vec::new(),
    }
}
