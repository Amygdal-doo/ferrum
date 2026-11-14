//! Technologies with a badge (brand colour, short mark) and the component
//! kind they usually are.

use super::{Category, Tech};

macro_rules! tech {
    ($id:literal, $name:literal, $cat:ident, $color:literal, $mark:literal, $kind:literal) => {
        Tech {
            id: $id,
            name: $name,
            category: Category::$cat,
            color: $color,
            mark: $mark,
            kind: $kind,
        }
    };
}

pub const TECHS: &[Tech] = &[
    // Mobile
    tech!(
        "ios",
        "iOS (Swift)",
        Mobile,
        0xf05138,
        "iOS",
        "mobile-client"
    ),
    tech!(
        "android",
        "Android (Kotlin)",
        Mobile,
        0x3ddc84,
        "And",
        "mobile-client"
    ),
    tech!(
        "react-native",
        "React Native",
        Mobile,
        0x61dafb,
        "RN",
        "mobile-client"
    ),
    tech!(
        "flutter",
        "Flutter",
        Mobile,
        0x02569b,
        "Fl",
        "mobile-client"
    ),
    tech!(
        "kmp",
        "Kotlin Multiplatform",
        Mobile,
        0x7f52ff,
        "KMP",
        "mobile-client"
    ),
    tech!("ionic", "Ionic", Mobile, 0x3880ff, "Io", "mobile-client"),
    tech!("maui", ".NET MAUI", Mobile, 0x512bd4, "MA", "mobile-client"),
    // Frontend
    tech!("react", "React", Frontend, 0x61dafb, "Re", "web-client"),
    tech!("nextjs", "Next.js", Frontend, 0x8b8b90, "N", "web-client"),
    tech!("angular", "Angular", Frontend, 0xdd0031, "Ng", "web-client"),
    tech!("vue", "Vue", Frontend, 0x42b883, "Vue", "web-client"),
    tech!("nuxt", "Nuxt", Frontend, 0x00dc82, "Nx", "web-client"),
    tech!("svelte", "Svelte", Frontend, 0xff3e00, "Sv", "web-client"),
    tech!("solid", "SolidJS", Frontend, 0x2c4f7c, "So", "web-client"),
    tech!("astro", "Astro", Frontend, 0xbc52ee, "As", "static-site"),
    tech!("remix", "Remix", Frontend, 0x8b8b90, "Rx", "web-client"),
    tech!("htmx", "htmx", Frontend, 0x3465a4, "hx", "web-client"),
    tech!("tauri", "Tauri", Frontend, 0xffc131, "Ta", "desktop-client"),
    tech!(
        "electron",
        "Electron",
        Frontend,
        0x47848f,
        "El",
        "desktop-client"
    ),
    // Backend
    tech!("nodejs", "Node.js", Backend, 0x5fa04e, "JS", "service"),
    tech!("nestjs", "NestJS", Backend, 0xe0234e, "Ne", "service"),
    tech!("bun", "Bun", Backend, 0xfbf0df, "Bun", "service"),
    tech!("dotnet", ".NET (C#)", Backend, 0x512bd4, "C#", "service"),
    tech!(
        "java",
        "Java (Spring Boot)",
        Backend,
        0x6db33f,
        "Jv",
        "service"
    ),
    tech!(
        "kotlin",
        "Kotlin (Ktor)",
        Backend,
        0x7f52ff,
        "Kt",
        "service"
    ),
    tech!("php", "PHP (Laravel)", Backend, 0x777bb4, "PHP", "service"),
    tech!(
        "python",
        "Python (FastAPI)",
        Backend,
        0x3776ab,
        "Py",
        "service"
    ),
    tech!("django", "Django", Backend, 0x0c4b33, "Dj", "service"),
    tech!("rust", "Rust (Axum)", Backend, 0xdea584, "Rs", "service"),
    tech!("go", "Go", Backend, 0x00add8, "Go", "service"),
    tech!("ruby", "Ruby on Rails", Backend, 0xcc0000, "Rb", "service"),
    tech!(
        "elixir",
        "Elixir (Phoenix)",
        Backend,
        0x6e4a7e,
        "Ex",
        "service"
    ),
    tech!("scala", "Scala", Backend, 0xdc322f, "Sc", "service"),
    tech!(
        "graphql",
        "GraphQL",
        Backend,
        0xe10098,
        "GQL",
        "api-gateway"
    ),
    tech!("grpc", "gRPC", Backend, 0x244c5a, "RPC", "service"),
    // Data
    tech!(
        "postgresql",
        "PostgreSQL",
        Database,
        0x4169e1,
        "Pg",
        "sql-db"
    ),
    tech!("mysql", "MySQL", Database, 0x4479a1, "My", "sql-db"),
    tech!(
        "sqlserver",
        "SQL Server",
        Database,
        0xcc2927,
        "MS",
        "sql-db"
    ),
    tech!(
        "oracle",
        "Oracle Database",
        Database,
        0xf80000,
        "Or",
        "sql-db"
    ),
    tech!("sqlite", "SQLite", Database, 0x0f80cc, "SL", "sql-db"),
    tech!("mongodb", "MongoDB", Database, 0x47a248, "Mg", "nosql-db"),
    tech!("dynamodb", "DynamoDB", Database, 0x4053d6, "Dy", "nosql-db"),
    tech!(
        "cassandra",
        "Cassandra",
        Database,
        0x1287b1,
        "Ca",
        "wide-column"
    ),
    tech!(
        "scylladb",
        "ScyllaDB",
        Database,
        0x6cd5e7,
        "Sy",
        "wide-column"
    ),
    tech!("redis", "Redis / Valkey", Database, 0xff4438, "Rd", "cache"),
    tech!(
        "clickhouse",
        "ClickHouse",
        Database,
        0xfaff69,
        "CH",
        "warehouse"
    ),
    tech!(
        "elasticsearch",
        "Elasticsearch",
        Database,
        0x00bfb3,
        "ES",
        "search"
    ),
    tech!(
        "opensearch",
        "OpenSearch",
        Database,
        0x005eb8,
        "OS",
        "search"
    ),
    tech!("memcached", "Memcached", Database, 0x8bc34a, "Mc", "cache"),
    tech!(
        "timescaledb",
        "TimescaleDB",
        Database,
        0xfdb515,
        "TS",
        "time-series"
    ),
    tech!(
        "minio",
        "MinIO / S3",
        Database,
        0xc72e49,
        "S3",
        "object-storage"
    ),
    // Messaging
    tech!("kafka", "Apache Kafka", Messaging, 0x8b8b90, "Kf", "stream"),
    tech!("rabbitmq", "RabbitMQ", Messaging, 0xff6600, "RMQ", "queue"),
    tech!("nats", "NATS", Messaging, 0x27aae1, "NAT", "pubsub"),
    tech!(
        "websocket",
        "WebSockets",
        Messaging,
        0x8b5cf6,
        "WS",
        "realtime"
    ),
    // AI
    tech!("claude", "Claude", Ai, 0xd97757, "Cl", "llm"),
    tech!("claude-code", "Claude Code", Ai, 0xd97757, "CC", "agent"),
    tech!("openai", "OpenAI", Ai, 0x10a37f, "AI", "llm"),
    tech!("gemini", "Gemini", Ai, 0x4285f4, "Gm", "llm"),
    tech!("deepseek", "DeepSeek", Ai, 0x4d6bfe, "DS", "llm"),
    tech!("kimi", "Kimi", Ai, 0x1677ff, "Ki", "llm"),
    tech!("muse", "Muse (Meta)", Ai, 0x0668e1, "Mu", "llm"),
    tech!("ollama", "Ollama", Ai, 0x8b8b90, "Ol", "llm"),
    tech!("vllm", "vLLM", Ai, 0x30a2ff, "vL", "llm"),
    tech!("qdrant", "Qdrant", Ai, 0xdc244c, "Qd", "vector-db"),
    tech!("pgvector", "pgvector", Ai, 0x4169e1, "pgv", "vector-db"),
    tech!("weaviate", "Weaviate", Ai, 0x00c8a0, "Wv", "vector-db"),
    tech!("chroma", "Chroma", Ai, 0xffde2d, "Ch", "vector-db"),
    tech!("pinecone", "Pinecone", Ai, 0x8b8b90, "Pc", "vector-db"),
    tech!("tensorflow", "TensorFlow", Ai, 0xff6f00, "TF", "ml"),
    tech!("pytorch", "PyTorch", Ai, 0xee4c2c, "PT", "ml"),
    tech!("huggingface", "Hugging Face", Ai, 0xffd21e, "HF", "ml"),
    tech!("langchain", "LangChain", Ai, 0x1c3c3c, "LC", "service"),
    tech!("llamaindex", "LlamaIndex", Ai, 0x8b5cf6, "LI", "service"),
    // Infrastructure
    tech!(
        "kubernetes",
        "Kubernetes",
        Infra,
        0x326ce5,
        "K8s",
        "cluster"
    ),
    tech!("docker", "Docker", Infra, 0x2496ed, "Dk", "container-app"),
    tech!("helm", "Helm", Infra, 0x0f1689, "Hm", "ci"),
    tech!("istio", "Istio", Infra, 0x466bb0, "Is", "service"),
    tech!("nginx", "Nginx", Infra, 0x009639, "Nx", "load-balancer"),
    tech!("envoy", "Envoy", Infra, 0xac6199, "Ev", "load-balancer"),
    tech!("traefik", "Traefik", Infra, 0x24a1c1, "Tr", "load-balancer"),
    tech!(
        "terraform",
        "Terraform / OpenTofu",
        Infra,
        0x7b42bc,
        "Tf",
        "ci"
    ),
    tech!(
        "prometheus",
        "Prometheus",
        Infra,
        0xe6522c,
        "Pr",
        "monitoring"
    ),
    tech!("grafana", "Grafana", Infra, 0xf46800, "Gf", "monitoring"),
    tech!(
        "opentelemetry",
        "OpenTelemetry",
        Infra,
        0xf5a800,
        "OT",
        "logging"
    ),
    tech!("vault", "Vault", Infra, 0xffd814, "Vt", "secrets"),
    tech!("keycloak", "Keycloak", Infra, 0x4d4d4d, "KC", "auth"),
    tech!(
        "github-actions",
        "GitHub Actions",
        Infra,
        0x2088ff,
        "GH",
        "ci"
    ),
    tech!("argocd", "Argo CD", Infra, 0xef7b4d, "Ar", "ci"),
    // Clouds
    tech!("aws", "AWS", Cloud, 0xff9900, "AWS", "external"),
    tech!("azure", "Azure", Cloud, 0x0078d4, "Az", "external"),
    tech!("gcp", "Google Cloud", Cloud, 0x4285f4, "GC", "external"),
    tech!("hetzner", "Hetzner", Cloud, 0xd50c2d, "Hz", "external"),
    tech!(
        "digitalocean",
        "DigitalOcean",
        Cloud,
        0x0080ff,
        "DO",
        "external"
    ),
    tech!("cloudflare", "Cloudflare", Cloud, 0xf38020, "CF", "cdn"),
    tech!("stripe", "Stripe", Cloud, 0x635bff, "St", "external"),
];
