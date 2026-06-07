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
7. [Manifest Format](#manifest-format)
8. [Validation Rules](#validation-rules)
9. [Agnostic Data API](#agnostic-data-api)

---

## Overview

Zen Garden uses curated service templates called "offerings" to ensure consistent, validated deployments. Each offering is a container definition (`<name>.snippet.yaml`) plus optional catalog metadata, compatibility rules, and post-install guidance.

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

Offerings are embedded in the Moss binary at build time and overlaid at runtime
from the filesystem. Each offering is a set of files keyed by name inside a
category folder:

```
src/moss/embedded/manifests/         # embedded at compile time (rust_embed)
└── sw/<category>/
    ├── <name>.snippet.yaml          # required — container definition
    ├── <name>.frontmatter.json      # catalog metadata
    ├── <name>.compatibility.yaml    # pre-flight rules
    └── <name>.guidance.md           # post-install notes

{data_dir}/manifests/                # runtime overlay (e.g. /var/lib/zen-garden/manifests)
└── sw/<category>/
    └── <name>.snippet.yaml          # a same-named file overrides the embedded copy
```

The offering's **name and category are derived from the file path**, not from
the file contents. The filesystem overlay takes precedence over the embedded
copy on the offering key; fields absent from the filesystem copy (guidance,
compatibility, connection, description, tags) are back-filled from the embedded
one.

**Registry loading (Moss startup):**

1. Load every embedded `sw/<category>/<name>.snippet.yaml` and its companion files
2. Overlay `{data_dir}/manifests/sw/` — filesystem copies win on the offering key
3. Validate each template (schema, syntax, injection checks)
4. Load compatibility rules from `<name>.compatibility.yaml` (if present)
5. Evaluate compatibility against Stone capabilities
6. Build the in-memory index with tags + compatibility decisions

**Refresh command:**

```bash
garden-rake offer refresh --at stone-01
```

Rebuilds the index after manifest files are added, removed, or edited.

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

An offering is defined by up to four sibling files sharing the `<name>` stem.
Only `<name>.snippet.yaml` is required; the rest enrich it.

### `<name>.snippet.yaml` — container definition

A Docker-Compose-style service body. Moss parses `image`, `ports` (map of role →
`[host, container]`; the `default` role is the primary port), `environment`,
`volumes`, `command`, `config_files`, `tasks` (with an optional `action: recycle`),
`healthcheck`, `network`, `deploy.resources.reservations.devices` (GPU), and
`deploy.resources.limits` (`memory`/`cpus`). Other Compose keys
(`container_name`, `restart`, `networks`) are accepted but ignored.

```yaml
# mongodb.snippet.yaml
image: mongo:7
ports:
  default: [27017, 27017]
environment:
  MONGO_INITDB_ROOT_USERNAME: ${MONGO_USER:-admin}
  MONGO_INITDB_ROOT_PASSWORD: ${MONGO_PASSWORD:-secret}
volumes:
  - mongo-data:/data/db
config_files:
  - path: /etc/mongod.conf
    format: yaml
    flag: "--config /etc/mongod.conf"
    reload: restart
```

### `<name>.frontmatter.json` — catalog metadata

```json
{
  "name": "mongodb",
  "description": "Document database with ACID transactions (SSPL)",
  "category": "data",
  "tags": ["database", "document", "nosql"],
  "port": 27017,
  "connection": { "protocol": "mongodb", "uri_template": "mongodb://{host}:{port}" },
  "coordination": "elected",
  "manageable_env": { "service_name": "mongodb", "vars": ["MONGO_INITDB_ROOT_USERNAME"] }
}
```

- `name` and `category` here are informational — the loader derives both from the file path.
- `connection.uri_template` uses `{host}` and `{port}` placeholders, filled at connect time.
- `coordination` is `independent` (default) or `elected` (Primary/Replica election for stateful offerings).
- `manageable_env` allowlists the env vars Moss may read/write via the `/env` endpoints.
- `ceremony` declares snapshot quiesce/resume hooks (ORCH-0041); `minimum_memory_gb` synthesizes a warn-only RAM rule.

### `<name>.compatibility.yaml` — pre-flight rules

Pre-flight `when:` predicate rules with optional per-host image `fallback`, plus
a `post_install_healthcheck` log-scan block. The full grammar and the
fact/operator reference live in the
[compatibility guide](../guides/offering-manifest-compatibility.md).

```yaml
# mongodb.compatibility.yaml
compatibility_rules:
  - name: "missing-avx-feature"
    when:
      - host.cpu.features LACKS avx
    reason: "MongoDB 5.0+ requires AVX CPU support"
    fallback:
      image: "mongo:4.4"
      name: "legacy"
```

### `<name>.guidance.md` — post-install notes

Markdown shown on the stone portrait page, with a `version`/`trigger`
frontmatter block and `{{template}}` variables. See
[guidance-authoring](../guides/guidance-authoring.md).

---

## Validation Rules

`garden-rake manifest validate <path>` (and Moss, before a test deployment) runs
the rules in `garden_common::manifests::validation`. Each finding carries a code
and a severity: an **Error** blocks the offering from loading; **Warning** and
**Info** are advisory.

**Snippet (`<name>.snippet.yaml`):**

| Code | Severity | Rule |
|------|----------|------|
| `YAML001` | Error | File is not valid YAML |
| `SCHEMA001` | Error | Missing `image` field |
| `SCHEMA002` | Error | `image` is empty |
| `SCHEMA003` | Info | No `ports` — offering is internal-only |
| `SEC001` | Error | `privileged: true` |
| `SEC002` | Error | `network_mode: host` |
| `SEC003` | Error | Volume mounts a sensitive host path (`/`, `/etc`, `/proc`, `/sys`, `/var/run/docker.sock`, `/dev`) |
| `SEC004` | Error | Port `0` |
| `SEC005` | Warning | Duplicate host port |

**Frontmatter (`<name>.frontmatter.json`):**

| Code | Severity | Rule |
|------|----------|------|
| `FM001` | Error | File is not valid JSON |
| `FM002` | Error | Missing `name` field |
| `FM003` | Warning | Missing `description` |
| `FM004` | Error | `port` outside 1–65535 |
| `FM005` | Warning | Unknown `category` (alias-aware; skipped when the registry is empty) |
| `FM006` | Warning | Frontmatter `port` ≠ snippet `ports.default` host port |
| `FM007` | Warning | Unknown top-level frontmatter key |

**Compatibility (`<name>.compatibility.yaml`):**

| Code | Severity | Rule |
|------|----------|------|
| `COMPAT001` | Error | File is not valid YAML |
| `COMPAT003` | Error | A `compatibility_rules[].when` predicate fails to parse |

A directory containing no manifest files yields `DIR001` (Warning). Any Error
finding blocks the offering; the daemon logs `Skipped N invalid manifest` and
continues loading the rest.

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
