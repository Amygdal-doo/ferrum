//! Component kinds and each provider's service for them: AWS, Azure, GCP,
//! Hetzner, DigitalOcean, open source. Where a provider has no managed
//! service the name says what to run instead.

use super::{Category, Kind};

macro_rules! kind {
    ($id:literal, $name:literal, $cat:ident, [$($s:literal),* $(,)?]) => {
        Kind { id: $id, name: $name, category: Category::$cat, services: [$($s),*] }
    };
}

pub const KINDS: &[Kind] = &[
    kind!(
        "web-client",
        "Web app",
        Client,
        [
            "Browser", "Browser", "Browser", "Browser", "Browser", "Browser"
        ]
    ),
    kind!(
        "mobile-client",
        "Mobile app",
        Mobile,
        [
            "iOS / Android app",
            "iOS / Android app",
            "iOS / Android app",
            "iOS / Android app",
            "iOS / Android app",
            "iOS / Android app"
        ]
    ),
    kind!(
        "desktop-client",
        "Desktop app",
        Client,
        [
            "Desktop app",
            "Desktop app",
            "Desktop app",
            "Desktop app",
            "Desktop app",
            "Desktop app"
        ]
    ),
    kind!(
        "dns",
        "DNS",
        Client,
        [
            "Route 53",
            "Azure DNS",
            "Cloud DNS",
            "Hetzner DNS",
            "DigitalOcean DNS",
            "CoreDNS"
        ]
    ),
    kind!(
        "cdn",
        "CDN",
        Client,
        [
            "CloudFront",
            "Azure Front Door",
            "Cloud CDN",
            "Bunny CDN (external)",
            "Spaces CDN",
            "Varnish"
        ]
    ),
    kind!(
        "waf",
        "Firewall / WAF",
        Client,
        [
            "AWS WAF",
            "Front Door WAF",
            "Cloud Armor",
            "Hetzner Firewall",
            "Cloud Firewall",
            "ModSecurity"
        ]
    ),
    kind!(
        "load-balancer",
        "Load balancer",
        Client,
        [
            "Elastic Load Balancing",
            "Application Gateway",
            "Cloud Load Balancing",
            "Hetzner Load Balancer",
            "DO Load Balancer",
            "HAProxy"
        ]
    ),
    kind!(
        "api-gateway",
        "API gateway",
        Client,
        [
            "API Gateway",
            "API Management",
            "Apigee",
            "Kong on Cloud Server",
            "Kong on Droplet",
            "Kong"
        ]
    ),
    kind!(
        "static-site",
        "Static site",
        Frontend,
        [
            "S3 + CloudFront",
            "Static Web Apps",
            "Firebase Hosting",
            "Object Storage site",
            "App Platform static",
            "Nginx"
        ]
    ),
    kind!(
        "service",
        "Application service",
        Backend,
        [
            "EKS service",
            "AKS service",
            "GKE service",
            "k3s on Cloud Servers",
            "DOKS service",
            "Kubernetes Deployment"
        ]
    ),
    kind!(
        "container-app",
        "Container app",
        Backend,
        [
            "ECS Fargate",
            "Container Apps",
            "Cloud Run",
            "Docker on Cloud Server",
            "App Platform",
            "Docker Compose"
        ]
    ),
    kind!(
        "worker",
        "Background worker",
        Backend,
        [
            "ECS worker",
            "Container Apps job",
            "Cloud Run job",
            "Worker on Cloud Server",
            "App Platform worker",
            "Worker Deployment"
        ]
    ),
    kind!(
        "function",
        "Serverless function",
        Backend,
        [
            "Lambda",
            "Azure Functions",
            "Cloud Functions",
            "OpenFaaS on k3s",
            "DO Functions",
            "Knative"
        ]
    ),
    kind!(
        "cron",
        "Scheduled job",
        Backend,
        [
            "EventBridge Scheduler",
            "Logic Apps",
            "Cloud Scheduler",
            "systemd timer",
            "App Platform job",
            "Kubernetes CronJob"
        ]
    ),
    kind!(
        "auth",
        "Identity / auth",
        Backend,
        [
            "Cognito",
            "Entra ID",
            "Identity Platform",
            "Keycloak on Cloud Server",
            "Keycloak on Droplet",
            "Keycloak"
        ]
    ),
    kind!(
        "queue",
        "Message queue",
        Messaging,
        [
            "SQS",
            "Service Bus",
            "Cloud Tasks",
            "RabbitMQ on Cloud Server",
            "RabbitMQ on Droplet",
            "RabbitMQ"
        ]
    ),
    kind!(
        "stream",
        "Event stream",
        Messaging,
        [
            "Kinesis / MSK",
            "Event Hubs",
            "Pub/Sub",
            "Kafka on Cloud Servers",
            "Managed Kafka",
            "Apache Kafka"
        ]
    ),
    kind!(
        "pubsub",
        "Pub/sub & notifications",
        Messaging,
        [
            "SNS",
            "Event Grid",
            "Pub/Sub",
            "NATS on Cloud Server",
            "NATS on Droplet",
            "NATS"
        ]
    ),
    kind!(
        "realtime",
        "Realtime gateway",
        Messaging,
        [
            "API Gateway WebSockets",
            "Web PubSub",
            "Cloud Run WebSockets",
            "Centrifugo on Cloud Server",
            "App Platform WebSockets",
            "Centrifugo"
        ]
    ),
    kind!(
        "email",
        "Email / push",
        Messaging,
        [
            "SES / SNS mobile push",
            "Communication Services",
            "Firebase Cloud Messaging",
            "Postal on Cloud Server",
            "SendGrid (external)",
            "Postal"
        ]
    ),
    kind!(
        "cache",
        "In-memory cache",
        Database,
        [
            "ElastiCache",
            "Azure Cache for Redis",
            "Memorystore",
            "Valkey on Cloud Server",
            "Managed Valkey",
            "Valkey"
        ]
    ),
    kind!(
        "sql-db",
        "Relational database",
        Database,
        [
            "RDS / Aurora",
            "Azure Database for PostgreSQL",
            "Cloud SQL",
            "PostgreSQL on Cloud Server",
            "Managed PostgreSQL",
            "PostgreSQL"
        ]
    ),
    kind!(
        "nosql-db",
        "Document database",
        Database,
        [
            "DynamoDB",
            "Cosmos DB",
            "Firestore",
            "MongoDB on Cloud Server",
            "Managed MongoDB",
            "MongoDB"
        ]
    ),
    kind!(
        "wide-column",
        "Wide-column database",
        Database,
        [
            "Keyspaces",
            "Cosmos DB for Cassandra",
            "Bigtable",
            "ScyllaDB on Cloud Servers",
            "ScyllaDB on Droplets",
            "ScyllaDB"
        ]
    ),
    kind!(
        "time-series",
        "Time-series database",
        Database,
        [
            "Timestream",
            "Azure Data Explorer",
            "Bigtable",
            "TimescaleDB on Cloud Server",
            "Managed PostgreSQL + Timescale",
            "TimescaleDB"
        ]
    ),
    kind!(
        "object-storage",
        "Object storage",
        Database,
        [
            "S3",
            "Blob Storage",
            "Cloud Storage",
            "Hetzner Object Storage",
            "Spaces",
            "MinIO"
        ]
    ),
    kind!(
        "search",
        "Search index",
        Database,
        [
            "OpenSearch Service",
            "Azure AI Search",
            "Vertex AI Search",
            "OpenSearch on Cloud Server",
            "Managed OpenSearch",
            "OpenSearch"
        ]
    ),
    kind!(
        "vector-db",
        "Vector database",
        Ai,
        [
            "OpenSearch vectors / Aurora pgvector",
            "Azure AI Search vectors",
            "Vertex AI Vector Search",
            "Qdrant on Cloud Server",
            "Managed PostgreSQL + pgvector",
            "Qdrant"
        ]
    ),
    kind!(
        "warehouse",
        "Data warehouse",
        Database,
        [
            "Redshift",
            "Synapse Analytics",
            "BigQuery",
            "ClickHouse on Cloud Server",
            "ClickHouse on Droplet",
            "ClickHouse"
        ]
    ),
    kind!(
        "etl",
        "Data pipeline",
        Database,
        [
            "Glue",
            "Data Factory",
            "Dataflow",
            "Airflow on Cloud Server",
            "Airflow on Droplet",
            "Apache Airflow"
        ]
    ),
    kind!(
        "llm",
        "LLM inference",
        Ai,
        [
            "Bedrock",
            "Azure OpenAI",
            "Vertex AI (Gemini)",
            "Ollama on GPU server",
            "GPU Droplet + vLLM",
            "Ollama / vLLM"
        ]
    ),
    kind!(
        "ml",
        "ML training / serving",
        Ai,
        [
            "SageMaker",
            "Azure Machine Learning",
            "Vertex AI",
            "GPU server + MLflow",
            "GPU Droplet + MLflow",
            "MLflow"
        ]
    ),
    kind!(
        "agent",
        "Coding / AI agent",
        Ai,
        [
            "Bedrock Agents",
            "Azure AI Agent Service",
            "Vertex AI Agent Builder",
            "Agent on Cloud Server",
            "Agent on Droplet",
            "Agent runtime"
        ]
    ),
    kind!(
        "cluster",
        "Kubernetes cluster",
        Infra,
        [
            "EKS",
            "AKS",
            "GKE",
            "k3s on Cloud Servers",
            "DOKS",
            "Kubernetes"
        ]
    ),
    kind!(
        "vm",
        "Virtual machine",
        Infra,
        [
            "EC2",
            "Virtual Machines",
            "Compute Engine",
            "Cloud Server",
            "Droplet",
            "KVM / Proxmox"
        ]
    ),
    kind!(
        "registry",
        "Container registry",
        Infra,
        [
            "ECR",
            "Container Registry",
            "Artifact Registry",
            "Harbor on Cloud Server",
            "DO Container Registry",
            "Harbor"
        ]
    ),
    kind!(
        "secrets",
        "Secrets",
        Infra,
        [
            "Secrets Manager",
            "Key Vault",
            "Secret Manager",
            "Vault on Cloud Server",
            "Vault on Droplet",
            "HashiCorp Vault / OpenBao"
        ]
    ),
    kind!(
        "monitoring",
        "Metrics & alerts",
        Infra,
        [
            "CloudWatch",
            "Azure Monitor",
            "Cloud Monitoring",
            "Prometheus + Grafana",
            "DO Monitoring",
            "Prometheus + Grafana"
        ]
    ),
    kind!(
        "logging",
        "Logs & traces",
        Infra,
        [
            "CloudWatch Logs / X-Ray",
            "Log Analytics",
            "Cloud Logging / Trace",
            "Loki + Tempo",
            "Loki on Droplet",
            "Loki + Tempo"
        ]
    ),
    kind!(
        "ci",
        "CI/CD",
        Infra,
        [
            "CodePipeline",
            "Azure Pipelines",
            "Cloud Build",
            "GitHub Actions",
            "GitHub Actions",
            "GitHub Actions / Argo CD"
        ]
    ),
    kind!(
        "external",
        "External service",
        Cloud,
        [
            "External API",
            "External API",
            "External API",
            "External API",
            "External API",
            "External API"
        ]
    ),
];
