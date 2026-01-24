---
audience: [developer, operator, contributor]
doc_type: reference
status: current
last_verified: 2026-01-24
canonical: true
---

# Service Catalog

Complete catalog of all service offerings available in Zen Garden. Each service has a manifest in `manifests/` with Docker Compose snippets and compatibility metadata.

**Total Offerings:** 28 services across 15 categories

---

## Quick Reference

| Category | Services |
|----------|----------|
| [AI](#ai) | Ollama |
| [Auth](#authentication) | Authelia |
| [Automation](#automation) | n8n |
| [Cache](#cache) | Memcached |
| [Dashboard](#dashboard) | Homepage |
| [Data](#data) | MongoDB, PostgreSQL, MariaDB, SQL Server, Redis, Elasticsearch, OpenSearch, Couchbase |
| [DevOps](#devops) | Registry |
| [Messaging](#messaging) | RabbitMQ, NATS |
| [Networking](#networking) | Pi-hole, WireGuard |
| [Observability](#observability) | Aspire Dashboard, Grafana, Prometheus |
| [Proxy](#proxy) | Traefik |
| [Secrets](#secrets) | Vault |
| [Storage](#storage) | MinIO, Nextcloud |
| [Time Series](#time-series) | InfluxDB |
| [Vector](#vector) | Weaviate, Milvus |

---

## AI

### Ollama

Local LLM runtime for running open-source models.

| Property | Value |
|----------|-------|
| **Image** | `ollama/ollama:latest` |
| **Port** | 11434 |
| **Volumes** | `/root/.ollama` (models) |
| **GPU** | NVIDIA passthrough via `--gpus all` |

```bash
rake plant ollama
```

---

## Authentication

### Authelia

Authentication and authorization server with 2FA support.

| Property | Value |
|----------|-------|
| **Image** | `authelia/authelia:4.38` |
| **Port** | 9091 |
| **Volumes** | `/config` |

```bash
rake plant authelia
```

---

## Automation

### n8n

Workflow automation platform with 400+ integrations.

| Property | Value |
|----------|-------|
| **Image** | `n8nio/n8n:1.72.1` |
| **Port** | 5678 |
| **Volumes** | `/home/node/.n8n` |

```bash
rake plant n8n
```

---

## Cache

### Memcached

High-performance distributed memory cache.

| Property | Value |
|----------|-------|
| **Image** | `memcached:alpine` |
| **Port** | 11211 |

```bash
rake plant memcached
```

---

## Dashboard

### Homepage

Modern, self-hosted application dashboard.

| Property | Value |
|----------|-------|
| **Image** | `ghcr.io/gethomepage/homepage:v0.9.13` |
| **Port** | 3000 (exposed as 3001) |
| **Volumes** | `/app/config` |
| **Features** | Docker integration, service auto-discovery |

```bash
rake plant homepage
```

---

## Data

### MongoDB

Document database for JSON-like data.

| Property | Value |
|----------|-------|
| **Image** | `mongo:7` |
| **Port** | 27017 |
| **Volumes** | `/data/db`, `/data/configdb` |
| **Connection** | `mongodb://[user:pass@]host:port[/database]` |

### PostgreSQL

Advanced open-source relational database.

| Property | Value |
|----------|-------|
| **Image** | `postgres:16` |
| **Port** | 5432 |
| **Volumes** | `/var/lib/postgresql/data` |
| **Connection** | `Host=host;Port=port;Database=db;Username=user;Password=pass` |

### MariaDB

MySQL-compatible relational database.

| Property | Value |
|----------|-------|
| **Image** | `mariadb:11` |
| **Port** | 3306 |
| **Volumes** | `/var/lib/mysql` |
| **Connection** | `Server=host;Port=port;Database=db;Uid=user;Pwd=pass` |

### SQL Server

Microsoft's enterprise relational database.

| Property | Value |
|----------|-------|
| **Image** | `mcr.microsoft.com/mssql/server:2022-latest` |
| **Port** | 1433 |
| **Volumes** | `/var/opt/mssql` |
| **Environment** | `ACCEPT_EULA=Y`, `SA_PASSWORD` |
| **Connection** | `Server=host,port;Database=db;User Id=sa;Password=pass;TrustServerCertificate=true` |

### Redis

In-memory data store for caching and messaging.

| Property | Value |
|----------|-------|
| **Image** | `redis/redis-stack:latest` |
| **Port** | 6379 |
| **Volumes** | `/data` |
| **Features** | RedisJSON, RediSearch included |

### Elasticsearch

Distributed search and analytics engine.

| Property | Value |
|----------|-------|
| **Image** | `elasticsearch:8.11.0` |
| **Ports** | 9200 (HTTP), 9300 (transport) |
| **Volumes** | `/usr/share/elasticsearch/data` |
| **Environment** | `discovery.type=single-node` |

### OpenSearch

Open-source search and analytics suite (Elasticsearch fork).

| Property | Value |
|----------|-------|
| **Image** | `opensearchproject/opensearch:latest` |
| **Port** | 9200 |
| **Volumes** | `/usr/share/opensearch/data` |

### Couchbase

Distributed NoSQL document database with built-in cache.

| Property | Value |
|----------|-------|
| **Image** | `couchbase:latest` |
| **Ports** | 8091 (web console), 11210 (data) |
| **Volumes** | `/opt/couchbase/var` |

```bash
rake plant mongodb
rake plant postgresql
rake plant mariadb
rake plant sqlserver
rake plant redis
rake plant elasticsearch
rake plant opensearch
rake plant couchbase
```

---

## DevOps

### Registry

Docker container image registry.

| Property | Value |
|----------|-------|
| **Image** | `registry:2` |
| **Port** | 5000 |
| **Volumes** | `/var/lib/registry` |

```bash
rake plant registry
```

---

## Messaging

### RabbitMQ

Feature-rich message broker with management UI.

| Property | Value |
|----------|-------|
| **Image** | `rabbitmq:3-management-alpine` |
| **Ports** | 5672 (AMQP), 15672 (management) |
| **Volumes** | `/var/lib/rabbitmq` |
| **Connection** | `amqp://user:pass@host:port/` |

### NATS

High-performance cloud-native messaging system.

| Property | Value |
|----------|-------|
| **Image** | `nats:alpine` |
| **Ports** | 4222 (client), 8222 (monitoring) |

```bash
rake plant rabbitmq
rake plant nats
```

---

## Networking

### Pi-hole

Network-wide ad blocking and local DNS.

| Property | Value |
|----------|-------|
| **Image** | `pihole/pihole:2024.07.0` |
| **Ports** | 53 (DNS), 80 (web UI as 8053) |
| **Volumes** | `/etc/pihole`, `/etc/dnsmasq.d` |
| **Requires** | `NET_ADMIN` capability |

### WireGuard

Fast, modern VPN tunnel.

| Property | Value |
|----------|-------|
| **Image** | `linuxserver/wireguard:latest` |
| **Port** | 51820 (UDP) |
| **Volumes** | `/config` |
| **Requires** | `NET_ADMIN`, `SYS_MODULE` capabilities |

```bash
rake plant pihole
rake plant wireguard
```

---

## Observability

### Aspire Dashboard

.NET Aspire's OpenTelemetry dashboard.

| Property | Value |
|----------|-------|
| **Image** | `mcr.microsoft.com/dotnet/aspire-dashboard:latest` |
| **Ports** | 18888 (dashboard), 4317 (OTLP) |

### Grafana

Visualization and analytics platform.

| Property | Value |
|----------|-------|
| **Image** | `grafana/grafana:latest` |
| **Port** | 3000 |
| **Volumes** | `/var/lib/grafana` |

### Prometheus

Metrics collection and alerting toolkit.

| Property | Value |
|----------|-------|
| **Image** | `prom/prometheus:latest` |
| **Port** | 9090 |
| **Volumes** | `/prometheus` |

```bash
rake plant aspire
rake plant grafana
rake plant prometheus
```

---

## Proxy

### Traefik

Cloud-native reverse proxy and load balancer.

| Property | Value |
|----------|-------|
| **Image** | `traefik:v3.2` |
| **Ports** | 80 (HTTP), 443 (HTTPS), 8080 (dashboard) |
| **Volumes** | `/var/run/docker.sock` (read-only) |
| **Features** | Automatic HTTPS, Docker provider |

```bash
rake plant traefik
```

---

## Secrets

### Vault

Secrets management and data protection.

| Property | Value |
|----------|-------|
| **Image** | `hashicorp/vault:latest` |
| **Port** | 8200 |
| **Volumes** | `/vault/data`, `/vault/logs` |
| **Note** | Dev mode only for stones; production requires unsealing |

```bash
rake plant vault
```

---

## Storage

### MinIO

S3-compatible object storage.

| Property | Value |
|----------|-------|
| **Image** | `minio/minio:RELEASE.2024-12-18T13-15-44Z` |
| **Ports** | 9000 (API), 9001 (console) |
| **Volumes** | `/data` |
| **Default credentials** | `minioadmin` / `minioadmin` |

### Nextcloud

Self-hosted file sync and share platform.

| Property | Value |
|----------|-------|
| **Image** | `nextcloud:latest` |
| **Port** | 80 |
| **Volumes** | `/var/www/html` |

```bash
rake plant minio
rake plant nextcloud
```

---

## Time Series

### InfluxDB

Time series database for metrics and events.

| Property | Value |
|----------|-------|
| **Image** | `influxdb:2.7-alpine` |
| **Port** | 8086 |
| **Volumes** | `/var/lib/influxdb2`, `/etc/influxdb2` |
| **Default org** | `zen-garden` |

```bash
rake plant influxdb
```

---

## Vector

### Weaviate

AI-native vector database for semantic search.

| Property | Value |
|----------|-------|
| **Image** | `semitechnologies/weaviate:latest` |
| **Port** | 8080 |
| **Volumes** | `/var/lib/weaviate` |

### Milvus

High-performance vector database for AI applications.

| Property | Value |
|----------|-------|
| **Image** | `milvusdb/milvus:latest` |
| **Ports** | 19530 (gRPC), 9091 (HTTP) |
| **Volumes** | `/var/lib/milvus` |
| **Note** | Standalone mode for dev; production requires etcd + MinIO |

```bash
rake plant weaviate
rake plant milvus
```

---

## Summary

| Category | Count |
|----------|:-----:|
| AI | 1 |
| Auth | 1 |
| Automation | 1 |
| Cache | 1 |
| Dashboard | 1 |
| Data | 8 |
| DevOps | 1 |
| Messaging | 2 |
| Networking | 2 |
| Observability | 3 |
| Proxy | 1 |
| Secrets | 1 |
| Storage | 2 |
| Time Series | 1 |
| Vector | 2 |
| **Total** | **28** |

---

## Manifest Structure

Each service in `manifests/<category>/` has:

| File | Purpose |
|------|---------|
| `<service>.snippet.yaml` | Docker Compose service definition |
| `<service>.compatibility.yaml` | Version matrix and framework support |

See [manifests/README.md](../../manifests/README.md) for authoring guidelines.
