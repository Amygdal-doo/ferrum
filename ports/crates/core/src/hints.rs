//! What a well-known port is usually used for.

/// A guess shown next to a port; never authoritative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hint {
    pub port: u16,
    /// One word for group summaries (`web`, `postgres`).
    pub short: &'static str,
    pub description: &'static str,
}

const fn h(port: u16, short: &'static str, description: &'static str) -> Hint {
    Hint {
        port,
        short,
        description,
    }
}

/// Sorted by port.
pub const HINTS: &[Hint] = &[
    h(80, "http", "HTTP"),
    h(443, "https", "HTTPS"),
    h(1025, "smtp", "MailHog / Mailpit SMTP"),
    h(1433, "mssql", "SQL Server"),
    h(2181, "zookeeper", "ZooKeeper"),
    h(2375, "docker", "Docker API"),
    h(3000, "web", "Next.js / Node / Rails / Grafana dev server"),
    h(3001, "web", "Node dev server (second app)"),
    h(3306, "mysql", "MySQL / MariaDB"),
    h(4000, "web", "Phoenix / Jekyll / GraphQL dev server"),
    h(4200, "angular", "Angular dev server"),
    h(4317, "otlp", "OpenTelemetry OTLP gRPC"),
    h(4318, "otlp", "OpenTelemetry OTLP HTTP"),
    h(4321, "astro", "Astro dev server"),
    h(
        5000,
        "web",
        "Flask / ASP.NET (macOS AirPlay Receiver also uses it)",
    ),
    h(5037, "adb", "Android Debug Bridge"),
    h(5050, "pgadmin", "pgAdmin"),
    h(5173, "vite", "Vite dev server"),
    h(5174, "vite", "Vite dev server (second app)"),
    h(5432, "postgres", "PostgreSQL"),
    h(5555, "prisma", "Prisma Studio"),
    h(5672, "rabbitmq", "RabbitMQ (AMQP)"),
    h(6006, "storybook", "Storybook"),
    h(6379, "redis", "Redis"),
    h(6380, "redis", "Redis (second instance)"),
    h(7000, "airplay", "macOS AirPlay Receiver / Cassandra"),
    h(7233, "temporal", "Temporal"),
    h(8000, "web", "Django / uvicorn / http.server"),
    h(8025, "mail", "MailHog / Mailpit web UI"),
    h(8080, "http-alt", "HTTP alternate (Tomcat, Spring, proxies)"),
    h(8081, "metro", "React Native Metro bundler"),
    h(8443, "https-alt", "HTTPS alternate"),
    h(8888, "jupyter", "Jupyter"),
    h(8983, "solr", "Solr"),
    h(9000, "web", "PHP-FPM / MinIO / SonarQube"),
    h(9090, "prometheus", "Prometheus"),
    h(9092, "kafka", "Kafka"),
    h(9200, "elastic", "Elasticsearch / OpenSearch"),
    h(9229, "inspector", "Node.js inspector (debugger)"),
    h(11434, "ollama", "Ollama"),
    h(15672, "rabbitmq", "RabbitMQ management UI"),
    h(16686, "jaeger", "Jaeger UI"),
    h(19000, "expo", "Expo dev server"),
    h(24678, "vite-hmr", "Vite HMR websocket"),
    h(27017, "mongo", "MongoDB"),
];

#[must_use]
pub fn hint(port: u16) -> Option<&'static Hint> {
    HINTS
        .binary_search_by_key(&port, |h| h.port)
        .ok()
        .map(|i| &HINTS[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_unique() {
        assert!(HINTS.windows(2).all(|w| w[0].port < w[1].port));
    }

    #[test]
    fn looks_up_ports() {
        assert_eq!(hint(5432).map(|h| h.short), Some("postgres"));
        assert_eq!(hint(9229).map(|h| h.short), Some("inspector"));
        assert!(hint(1).is_none());
    }
}
