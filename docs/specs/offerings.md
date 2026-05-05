# Service Offerings Specification

**Purpose:** Technical specification for service templates, taxonomy, and query-based recommendations.  
**Audience:** Developers implementing offering system, operators creating custom offerings.

---

## Table of Contents

1. [Overview](#overview)
2. [Offering Instances (FQN)](#offering-instances-fqn)
3. [Taxonomy and Query Recommendations](#taxonomy-and-query-recommendations)
4. [Offering Registry Structure](#offering-registry-structure)
5. [Native vs Agnostic Services](#native-vs-agnostic-services)
6. [Service Discovery](#service-discovery)
7. [Template Format](#template-format)
8. [Validation Rules](#validation-rules)
9. [Agnostic Data API](#agnostic-data-api)

---

## Overview

Zen Garden uses curated service templates called "offerings" to ensure consistent, validated deployments. Each offering defines both native and optional agnostic sidecar configurations.

**Key Distinction:**
- **Protocol** = Wire format for access (mongodb, s3, redis, storage)
- **Offering** = Software that implements protocols (MongoDB, MinIO, Redis)

Offerings declare which protocols they support. Resolution matches protocols to offerings.

**Design philosophy:**

- **Template-driven:** Prevent ad-hoc Docker configurations
- **Curated:** Maintained offerings with tested compatibility
- **Query-based:** Discover services by intent, not exact name
- **Protocol-aware:** Match by wire format (s3) or by software (minio)
- **Compatibility-aware:** Match offerings to Stone hardware automatically

---

## Offering Instances (FQN)

Zen Garden separates **offering type** from **instance identity** using a fully-qualified name (FQN):

```
offering[::instance]
```

- `ollama` → default instance
- `ollama::dev` → named instance

The FQN is used for service identity (registry, APIs, containers). The offering type is still used for manifest lookup and compatibility.  
See [offering-fqn.md](offering-fqn.md) for full rules and encoding details.

---

## Taxonomy and Query Recommendations

Offerings include lightweight metadata used for discovery and recommendations.

### Metadata Fields

- **Category:** Single stable category (e.g., `data`, `cache`, `search`, `vector`, `messaging`)
- **Tags:** Short lowercase tokens describing intent (e.g., `database`, `document`, `sql`, `nosql`)
- **Synonym dictionary:** `manifests/taxonomy.dictionary.yaml` maps user tokens to canonical tokens

**Example synonym mappings:**

```yaml
# manifests/taxonomy.dictionary.yaml
db: database
doc: document
docs: document
nosql: nosql
sql: sql
mq: messaging
queue: messaging
fts: search
vector: vector
ml: inference
```

### Query-Based Recommendations

Rake uses category + tags + compatibility to provide ranked recommendations:

```bash
# Query for document databases
garden-rake offer database,document

# Output:
# 1. mongodb (PASS on stone-01) - Category: data, Tags: database,document,nosql
# 2. couchdb (PASS on stone-01) - Category: data, Tags: database,document,nosql
# 3. arangodb (FALLBACK on stone-01) - Category: data, Tags: database,document,graph
```

### Cross-Stone Recommendations

```bash
# Find best Stone for vector database
garden-rake offer vector --at anywhere --prefer ssd,high-memory

# Ranks (stone, offering) pairs across discovered Stones
```

**Ranking logic:**

1. **Category match** > **Tag match** (category is authoritative)
2. **Compatibility status**: `PASS` > `FALLBACK` > `FAIL` (fails excluded)
3. **Prefer scoring**: SSD boost, high-memory boost, etc.
4. **Stone health**: Healthy > Degraded
5. **Resource availability**: More free RAM/disk = higher rank

**Compatibility failures excluded:** Offerings marked `FAIL` never appear in recommendations.

---

## Offering Registry Structure

```
/var/lib/zen-garden/manifests/
├── mongodb.yml
├── postgresql.yml
├── redis.yml
├── sqlserver.yml
├── mysql.yml
├── weaviate.yml
├── qdrant.yml
├── rabbitmq.yml
└── custom/
    └── user-defined-app.yml
```

**Registry loading:**

1. Scan `/var/lib/zen-garden/manifests/` on Moss startup
2. Validate each template (schema, syntax, injection checks)
3. Load compatibility rules from `.compatibility.yaml` (if present)
4. Evaluate compatibility against Stone capabilities
5. Build in-memory index with tags + compatibility decisions

**Refresh command:**

```bash
garden-rake offer refresh --at stone-01
```

Rebuilds index when templates modified (add/remove/edit frontmatter).

---

## Native vs Agnostic Services

### Native Service

Database/service on its native protocol:

- **Examples:** MongoDB (port 27017), PostgreSQL (5432), Redis (6379)
- **Uses:** Vendor-specific drivers
- **Features:** Full feature set available
- **Performance:** Best (no HTTP overhead)

### Agnostic Sidecar

HTTP REST API wrapping native service:

- **Port:** 8080+ (auto-assigned)
- **API:** Database-neutral HTTP (Koan EntityController patterns)
- **Purpose:** Backend portability

**Sidecars are per-service, not shared:**

- Stone running MongoDB + SQL Server = 2 sidecars
- Each sidecar dedicated to its parent service
- Independent port allocation per sidecar

---

## Service Discovery

URIs follow the [URI-0003](../decisions/URI-0003-zen-garden-urn-form-scheme.md) grammar. The discovery-side resolution algorithm is in [specs/discovery.md](discovery.md). This section describes how *offerings* fit into that scheme.

### Three ways to address an offering

**1. By name** — bare name; cascade hits the offering kind first.

```
zen-garden:mongodb              → the MongoDB offering
zen-garden:mongodb:staging      → its "staging" instance
zen-garden:mongodb/myapp        → with database sub-path
zen-garden:minio                → the MinIO offering (uses S3 wire protocol)
```

This is the dominant case. The cascade resolves to whichever offering has the matching name, and the offering's manifest determines the wire protocol.

**2. By capability** — empty target, `cap=` query; bypasses cascade.

```
zen-garden:?cap=s3              → any offering speaking S3 (MinIO, seed-bank gateway, etc.)
zen-garden:?cap=storage         → any offering speaking the agnostic storage API
zen-garden:?cap=mongodb         → any offering speaking the MongoDB wire protocol
```

This expresses "I want something speaking this protocol" without naming the offering. The resolver matches the `protocols` TXT record across all advertised offerings.

**3. By category** — bare name; cascade falls through to the category index when no offering matches.

```
zen-garden:database             → any offering tagged "database" in its taxonomy
zen-garden:document-database    → MongoDB / CouchDB / similar
zen-garden:vector               → Weaviate / Qdrant / similar
```

Category names are not reserved keywords. They live in `garden-common::constants::categories` and are consulted as the final cascade stage. The first seven kinds (offering, stone, bank, service, companion, pond, garden) are tried first; a category match is the fallback.

### Combining

Query parameters compose with any of the three forms:

```
zen-garden:mongodb?cap=mongodb              → name + capability constraint
zen-garden:mongodb?action=wish              → find-or-provision
zen-garden:?cap=s3&at=seed-usb-01           → capability pinned to a specific bank
zen-garden:database?tags=document           → category + taxonomy filter
zen-garden:offering//mongodb                → explicit offering kind (force offering cascade level)
```

### Resolution

Defer to the discovery-layer algorithm in [specs/discovery.md §"Connection String Resolution"](discovery.md#connection-string-resolution). In short:

1. Parse URI per URI-0003
2. Build candidate set (cascade, explicit kind, or capability-only)
3. Apply query constraints (`at=`, `cap=`, `tags=`, `protocol=`)
4. Filter by instance qualifier if present
5. Rank by health → priority → latency
6. Apply `?action=` if present (`wish` triggers provisioning)
7. Build native connection string from selected endpoint

---

## Manifest Format

### Example: MongoDB Offering

```yaml
# /var/lib/zen-garden/manifests/mongodb.yaml
---
name: mongodb
offering: mongodb
category: data
tags: [database, document, nosql, transactions]
description: Document database with ACID transactions and aggregation pipeline

versions:
  default: "7.0"
  supported:
    - "7.0"
    - "6.0"
    - "5.0"

# Protocols this offering supports
protocols:
  - port: 27017
    protocol: mongodb
    default: true
  - port: 8080
    protocol: agnostic
    sidecar: mongodb-agnostic

docker:
  native:
    offering_name: mongodb
    image: mongo
    image_tag: ${VERSION}
    ports:
      - container: 27017
        host: 27017
        protocol: tcp
    volumes:
      - name: mongodb_data
        mount: /data/db
        type: volume
    environment:
      MONGO_INITDB_ROOT_USERNAME: ${MONGO_USER:-admin}
      MONGO_INITDB_ROOT_PASSWORD: ${MONGO_PASSWORD:-secret}
    health_check:
      test: ["CMD", "mongosh", "--eval", "db.adminCommand('ping')"]
      interval: 30s
      timeout: 10s
      retries: 3

  agnostic:
    offering_name: mongodb-agnostic
    image: sylin/agnostic-mongodb
    image_tag: ${VERSION}
    ports:
      - container: 8080
        host_start: 8080
        protocol: tcp
    environment:
      BACKEND_SERVICE: mongodb
      BACKEND_URL: mongodb://mongodb:27017
      SET_MODE: database
    depends_on:
      - mongodb

mdns:
  - offering: mongodb
    protocol: native
    port_source: 27017
    categories: database,document-database
    capabilities: []
  - offering: mongodb-agnostic
    protocol: agnostic
    port_source: 8080
    categories: database,document-database
    capabilities: [crud, query, filter, bulk, transactions]
```

### Compatibility Rules

Optionally, create `mongodb.compatibility.yaml`:

```yaml
# mongodb.compatibility.yaml
rules:
  - condition:
      processor: arm64
    action: override_image
    value: mongo:7.0
    reason: "ARM64 requires official ARM image"

  - condition:
      processor: x86_64
      feature_missing: avx
    action: warn
    message: "MongoDB may perform poorly without AVX instructions"
```

---

## Validation Rules

Templates must pass validation before loading:

1. **Required fields:** `name`, `offering`, `docker.native` sections present
2. **Image tags:** Match regex `^[a-z0-9-_./]+:[a-z0-9-_.]+$` (no shell injection)
3. **Port numbers:** Range 1-65535, no duplicates
4. **Environment variables:** Only `${VAR}` or `${VAR:-default}` syntax
5. **Volume names:** Match regex `^[a-z0-9-]+$`
6. **No shell commands:** No arbitrary command execution in config

**Validation errors:**

```
✗ Template validation failed: mongodb.yml

Errors:
  - Invalid port: 70000 (must be 1-65535)
  - Invalid volume name: "mongo-data!" (only lowercase letters, numbers, hyphens)
  - Unsupported environment variable syntax: $(whoami)

Skipping mongodb.yml
```

---

## Agnostic Data API

**Status:** Future implementation (documented for completeness)

### Overview

Optional HTTP REST API providing database-neutral access to services. Based on Koan EntityController patterns.

### URL Structure

```
/v1/data/{set}/entities/{type}
/v1/data/{set}/entities/{type}/{id}
```

**Pattern enforces security:** Version + set + model prevents injection attacks.

### Endpoints

```http
# CRUD Operations
GET    /v1/data/{set}/entities/{type}        # List with pagination
GET    /v1/data/{set}/entities/{type}/{id}   # Get by ID
POST   /v1/data/{set}/entities/{type}        # Create
PUT    /v1/data/{set}/entities/{type}/{id}   # Update
DELETE /v1/data/{set}/entities/{type}/{id}   # Delete

# Advanced Operations
POST   /v1/data/{set}/entities/{type}/query  # Filter query
POST   /v1/data/{set}/entities/{type}/bulk   # Bulk upsert

# Discovery
GET    /v1/data/sets                         # List sets
GET    /v1/data/sets/{set}/entities          # List entity types
```

### Query Filter Syntax

Based on Koan JsonFilterBuilder (MongoDB-like):

```json
POST /v1/data/myapp/entities/users/query
{
  "filter": {
    "age": { "$gte": 18 },
    "status": "active",
    "email": { "$exists": true }
  },
  "sort": [{ "field": "createdAt", "descending": true }],
  "page": 1,
  "pageSize": 25
}
```

**Supported operators:**

- Comparison: `$gte`, `$lte`, `$gt`, `$lt`
- Arrays: `$in`, `$all`
- Logical: `$and`, `$or`, `$not`
- Special: `$exists`, wildcards (`Al*`)

### Set-Based Isolation

Sets map to backend namespaces:

- **MongoDB (database mode):** Each set = separate database
- **MongoDB (collection mode):** Each set = collection prefix
- **PostgreSQL/SQL Server:** Each set = schema
- **Redis:** Each set = keyspace prefix

**Example:**

```http
POST /v1/data/myapp/entities/users
→ MongoDB: db.myapp.users.insertOne(...)
→ PostgreSQL: INSERT INTO myapp.users ...
→ Redis: HSET myapp:users:123 ...
```

### Pagination

All list/query endpoints return pages by default:

```http
GET /v1/data/myapp/entities/users?page=2&pageSize=50&sort=-createdAt

Response Headers:
X-Page: 2
X-Page-Size: 50
X-Total-Count: 1247
X-Total-Pages: 25
```

### Bulk Operations

```json
POST /v1/data/myapp/entities/users/bulk
[
  { "id": "1", "name": "Alice", "age": 30 },
  { "id": "2", "name": "Bob", "age": 25 }
]

Response:
{
  "created": 1,
  "updated": 1,
  "errors": []
}
```

---

## Next Steps

- **Moss daemon specification:** [moss-daemon-lifecycle.md](moss-daemon-lifecycle.md)
- **Rake CLI specification:** [rake-commands.md](rake-commands.md)
- **Discovery protocol:** [discovery.md](discovery.md)
- **Creating custom offerings:** [../guides/offering-lifecycle.md](../guides/offering-lifecycle.md)
