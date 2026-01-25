# Zen Garden AWS Bridge Specification

**AWS-compatible APIs for your homelab cloud**

**Status:** Proposal  
**Date:** January 2026  
**Authors:** Collaborative design session

---

## Table of Contents

1. [Overview](#overview)
2. [Philosophy](#philosophy)
3. [Architecture](#architecture)
4. [Protocol Handlers](#protocol-handlers)
5. [Services](#services)
   - [S3 (Storage)](#s3-storage)
   - [SQS (Queues)](#sqs-queues)
   - [DynamoDB (NoSQL)](#dynamodb-nosql)
   - [Secrets Manager](#secrets-manager)
   - [Lambda (Functions)](#lambda-functions)
   - [SNS (Pub/Sub)](#sns-pubsub)
   - [SES (Email)](#ses-email)
   - [CloudWatch Logs](#cloudwatch-logs)
   - [Parameter Store (SSM)](#parameter-store-ssm)
   - [KMS (Key Management)](#kms-key-management)
6. [Backend Adapters](#backend-adapters)
7. [Auto-Provisioning](#auto-provisioning)
8. [Managed Offerings](#managed-offerings)
9. [Bridge UI](#bridge-ui)
10. [Bridge API](#bridge-api)
11. [CLI Integration](#cli-integration)
12. [Backup and Recovery](#backup-and-recovery)
13. [Configuration](#configuration)
14. [Security](#security)
15. [Offering Manifest](#offering-manifest)

---

## Overview

### What is the AWS Bridge?

The **AWS Bridge** is a Zen Garden offering that provides AWS-compatible APIs backed by garden-native services. It allows applications written for AWS to run unchanged on your homelab.

```
┌─────────────────────────────────────────────────────────────────┐
│                    AWS BRIDGE CONCEPT                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Your Application Code                                         │
│   ─────────────────────                                         │
│   const s3 = new S3Client({ endpoint: "zen-garden:s3//" });     │
│   const sqs = new SQSClient({ endpoint: "zen-garden:sqs//" });  │
│   const dynamo = new DynamoDBClient({ endpoint: "zen-garden:dynamodb//" });
│                                                                 │
│   SAME CODE. RUNS ON AWS. RUNS ON YOUR HOMELAB.                 │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Why?

| Problem | Solution |
|---------|----------|
| Local dev uses mocks | Real AWS APIs with real backends |
| LocalStack is single-node | Distributed across multiple stones |
| Cloud dev is expensive | $0 on your own hardware |
| Data in the cloud | Data on your network |
| Vendor lock-in | Standard APIs, portable backends |
| Complex setup | One command: `garden-rake offer zg-aws-bridge` |

### Key Features

1. **AWS API Compatibility** — Use official AWS SDKs unchanged
2. **Auto-Provisioning** — Bridge creates backend offerings as needed
3. **Self-Service UI** — Enable/disable services visually
4. **Fully Backed Up** — All backends cultivated to seed banks
5. **Distributed** — Services spread across stones intelligently
6. **Pond Integration** — Secrets and KMS when security enabled

---

## Philosophy

### The Local Cloud

Zen Garden with AWS Bridge becomes a **local cloud** — not a simulation, but a genuine distributed system with cloud-compatible APIs running on physical hardware you own.

### Real, Not Mocked

The bridge doesn't mock AWS services. It provides **real implementations** with different backends:

| AWS Service | Bridge Backend | Reality |
|-------------|---------------|---------|
| SQS | Redis | Real queue, real BRPOP, real persistence |
| DynamoDB | MongoDB | Real database, real queries, real indexes |
| Lambda | Containers | Real execution, real isolation, real resources |
| S3 | Seed Banks | Real files, real storage, real redundancy |

### Progressive Enhancement

Start simple, add services as needed:

```
Day 1:  garden-rake offer zg-aws-bridge
        → S3 available (uses existing seed bank)

Day 2:  Enable SQS in UI
        → Bridge provisions Redis
        → Queues work

Day 3:  Enable DynamoDB in UI
        → Bridge provisions MongoDB
        → NoSQL tables work

Day 4:  Enable Pond
        → Secrets Manager activates
        → KMS activates
```

### Everything is an Offering

The bridge and its backends are all offerings:

- Backed up automatically via cultivation
- Distributed across stones via wishes
- Recoverable via hydration
- Visible in garden topology

---

## Architecture

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    AWS BRIDGE ARCHITECTURE                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Client Application                                            │
│       │                                                         │
│       │ "zen-garden:sqs//my-app"                                │
│       ▼                                                         │
│   Zen Garden SDK                                                │
│       │                                                         │
│       │ PROTOCOL_QUERY: "who handles zen-garden:sqs?"           │
│       ▼                                                         │
│   Garden Discovery (UDP / Lantern)                              │
│       │                                                         │
│       │ PROTOCOL_RESPONSE: stone-jade-lake:4101                 │
│       ▼                                                         │
│   zg-aws-bridge (on stone-jade-lake)                            │
│       │                                                         │
│       │ Translates AWS SQS API → Redis commands                 │
│       ▼                                                         │
│   Redis Offering (on stone-silver-stream)                       │
│       │                                                         │
│       │ Cultivated to seed bank                                 │
│       ▼                                                         │
│   Seed Bank (USB / NAS)                                         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Component Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                       zg-aws-bridge                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌─────────────┐ ┌─────────────┐ ┌─────────────┐               │
│   │ S3 Adapter  │ │ SQS Adapter │ │DynamoDB     │               │
│   │ Port: 4100  │ │ Port: 4101  │ │Adapter 4102 │  ...          │
│   └──────┬──────┘ └──────┬──────┘ └──────┬──────┘               │
│          │               │               │                      │
│   ┌──────┴───────────────┴───────────────┴──────┐               │
│   │              Backend Router                  │               │
│   └──────┬───────────────┬───────────────┬──────┘               │
│          │               │               │                      │
│          ▼               ▼               ▼                      │
│   ┌────────────┐  ┌────────────┐  ┌────────────┐                │
│   │ Seed Bank  │  │   Redis    │  │  MongoDB   │                │
│   │ (S3)       │  │  (SQS,SNS) │  │ (DynamoDB) │                │
│   └────────────┘  └────────────┘  └────────────┘                │
│                                                                 │
│   ┌─────────────────────────────────────────────┐               │
│   │           Management Layer                   │               │
│   │  - Service enable/disable                    │               │
│   │  - Backend provisioning                      │               │
│   │  - Health monitoring                         │               │
│   │  - Protocol announcements                    │               │
│   └─────────────────────────────────────────────┘               │
│                                                                 │
│   ┌─────────────────────────────────────────────┐               │
│   │           Web UI (Port 4199)                 │               │
│   └─────────────────────────────────────────────┘               │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Startup Sequence

```
┌─────────────────────────────────────────────────────────────────┐
│              ZG-AWS-BRIDGE STARTUP SEQUENCE                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   1. Container starts                                           │
│                                                                 │
│   2. Load state from /var/zg-aws-bridge/state.yaml              │
│      (Restored from backup if hydrating)                        │
│                                                                 │
│   3. Discover available backends in garden:                     │
│      → "Who has redis?"      → stone-silver-stream              │
│      → "Who has mongodb?"    → stone-jade-lake                  │
│      → "Who has postgresql?" → not found                        │
│      → "Seed banks?"         → seed-glorious-dawn               │
│      → "Pond enabled?"       → yes/no                           │
│                                                                 │
│   4. For each enabled service:                                  │
│      a. Check backend available                                 │
│      b. If missing and auto_provision: wish for backend         │
│      c. If missing and !auto_provision: mark degraded           │
│      d. If available: connect and verify                        │
│                                                                 │
│   5. Start service adapters on their ports:                     │
│      S3       → 4100                                            │
│      SQS      → 4101                                            │
│      DynamoDB → 4102                                            │
│      Secrets  → 4103                                            │
│      Lambda   → 4104                                            │
│      SNS      → 4105                                            │
│      SES      → 4106                                            │
│      Logs     → 4107                                            │
│      SSM      → 4108                                            │
│      KMS      → 4109                                            │
│                                                                 │
│   6. Announce protocol handlers:                                │
│      PROTOCOL_HANDLER: zen-garden:s3       @ :4100              │
│      PROTOCOL_HANDLER: zen-garden:sqs      @ :4101              │
│      PROTOCOL_HANDLER: zen-garden:dynamodb @ :4102              │
│      ...                                                        │
│                                                                 │
│   7. Start Web UI on port 4199                                  │
│                                                                 │
│   8. Ready                                                      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Protocol Handlers

### Concept

A **protocol handler** is an offering that serves a specific `zen-garden:{protocol}//` connection string. The AWS Bridge registers handlers for all AWS-compatible protocols it supports.

### Registration

When the bridge enables a service, it announces a protocol handler:

```json
{
  "type": "PROTOCOL_HANDLER",
  "protocol": "zen-garden:sqs",
  "stone": "stone-jade-lake",
  "offering": "zg-aws-bridge",
  "port": 4101,
  "priority": 100,
  "healthy": true,
  "capabilities": [
    "SendMessage",
    "ReceiveMessage",
    "DeleteMessage",
    "CreateQueue",
    "DeleteQueue",
    "ListQueues",
    "GetQueueAttributes",
    "SetQueueAttributes"
  ]
}
```

### Resolution Protocol

```
Client                          Garden                         Bridge
   │                              │                              │
   │  PROTOCOL_QUERY              │                              │
   │  protocol: "zen-garden:sqs"  │                              │
   │  from: "my-app"              │                              │
   │─────────────────────────────>│                              │
   │                              │                              │
   │                              │  (lookup announcements)      │
   │                              │                              │
   │  PROTOCOL_RESPONSE           │                              │
   │  handlers: [                 │                              │
   │    { stone: stone-jade-lake, │                              │
   │      port: 4101,             │                              │
   │      healthy: true }         │                              │
   │  ]                           │                              │
   │<─────────────────────────────│                              │
   │                              │                              │
   │  AWS SQS API call            │                              │
   │  POST /queues/my-app/jobs    │                              │
   │────────────────────────────────────────────────────────────>│
   │                              │                              │
   │                              │              (translate &    │
   │                              │               execute)       │
   │                              │                              │
   │  SQS-compatible response     │                              │
   │<────────────────────────────────────────────────────────────│
```

### UDP Message Formats

**Query:**

```json
{
  "type": "PROTOCOL_QUERY",
  "protocol": "zen-garden:sqs",
  "from_stone": "stone-morning-mist",
  "from_app": "my-app",
  "request_id": "req-abc123"
}
```

**Response:**

```json
{
  "type": "PROTOCOL_RESPONSE",
  "request_id": "req-abc123",
  "protocol": "zen-garden:sqs",
  "handlers": [
    {
      "stone": "stone-jade-lake",
      "offering": "zg-aws-bridge",
      "endpoint": "http://stone-jade-lake.local:4101",
      "port": 4101,
      "priority": 100,
      "healthy": true,
      "version": "1.0.0"
    }
  ]
}
```

### Multiple Handlers

Multiple bridges can handle the same protocol for redundancy:

```json
{
  "handlers": [
    { "stone": "stone-jade-lake", "port": 4101, "priority": 100 },
    { "stone": "stone-silver-stream", "port": 4101, "priority": 50 }
  ]
}
```

Clients pick by priority, health, or proximity. Failover is automatic.

---

## Services

### Service Summary

| Service | Protocol | Port | Backend | Pond Required |
|---------|----------|------|---------|---------------|
| S3 | `zen-garden:s3` | 4100 | Seed Bank | No |
| SQS | `zen-garden:sqs` | 4101 | Redis / RabbitMQ | No |
| DynamoDB | `zen-garden:dynamodb` | 4102 | MongoDB / SQLite | No |
| Secrets Manager | `zen-garden:secrets` | 4103 | Keystone | **Yes** |
| Lambda | `zen-garden:lambda` | 4104 | Container Runtime | No |
| SNS | `zen-garden:sns` | 4105 | Redis / Internal | No |
| SES | `zen-garden:ses` | 4106 | Mailpit / SMTP | No |
| CloudWatch Logs | `zen-garden:logs` | 4107 | Loki / Files | No |
| Parameter Store | `zen-garden:ssm` | 4108 | Garden Config | No |
| KMS | `zen-garden:kms` | 4109 | Keystone | **Yes** |

---

### S3 (Storage)

**Protocol:** `zen-garden:s3//{app-name}[@{seed-bank}]`

**Backend:** Seed Banks (see [Storage API Specification](zen-garden-spec-storage-api.md))

**Description:** Object storage using the existing seed bank infrastructure. This service is automatically available when a seed bank exists.

**API Compatibility:**

| Operation | Supported | Notes |
|-----------|-----------|-------|
| PutObject | ✓ | |
| GetObject | ✓ | Range requests supported |
| DeleteObject | ✓ | |
| HeadObject | ✓ | |
| ListObjectsV2 | ✓ | Pagination supported |
| CopyObject | ✓ | |
| CreateMultipartUpload | ✓ | |
| UploadPart | ✓ | |
| CompleteMultipartUpload | ✓ | |
| AbortMultipartUpload | ✓ | |
| Presigned URLs | ✓ | Pond only |

**Connection:**

```javascript
const s3 = new S3Client({
  endpoint: await zenGarden.resolve("zen-garden:s3//my-app"),
  credentials: { accessKeyId: "zen-garden", secretAccessKey: "zen-garden" },
  region: "zen-garden",
  forcePathStyle: true,
});

await s3.send(new PutObjectCommand({
  Bucket: "garden",
  Key: "data/file.txt",
  Body: "Hello, World!",
}));
```

**Namespace:** Apps write to `apps/{app-name}/`. System (cultivation) writes to `garden/`.

---

### SQS (Queues)

**Protocol:** `zen-garden:sqs//{app-name}`

**Backends:** Redis (recommended), RabbitMQ, Built-in

**Description:** Message queuing for async processing, job scheduling, and service decoupling.

**API Compatibility:**

| Operation | Supported | Notes |
|-----------|-----------|-------|
| SendMessage | ✓ | |
| SendMessageBatch | ✓ | |
| ReceiveMessage | ✓ | Long polling supported |
| DeleteMessage | ✓ | |
| DeleteMessageBatch | ✓ | |
| CreateQueue | ✓ | |
| DeleteQueue | ✓ | |
| ListQueues | ✓ | |
| GetQueueUrl | ✓ | |
| GetQueueAttributes | ✓ | |
| SetQueueAttributes | ✓ | |
| PurgeQueue | ✓ | |
| ChangeMessageVisibility | ✓ | |

**Queue URL Format:**

```
http://stone.local:4101/sqs/{app-name}/{queue-name}

Example:
http://stone-jade-lake.local:4101/sqs/my-app/jobs
```

**Connection:**

```javascript
const sqs = new SQSClient({
  endpoint: await zenGarden.resolve("zen-garden:sqs//my-app"),
  credentials: { accessKeyId: "zen-garden", secretAccessKey: "zen-garden" },
  region: "zen-garden",
});

// Send message
await sqs.send(new SendMessageCommand({
  QueueUrl: "http://stone-jade-lake.local:4101/sqs/my-app/jobs",
  MessageBody: JSON.stringify({ task: "process", id: 123 }),
}));

// Receive message (long polling)
const response = await sqs.send(new ReceiveMessageCommand({
  QueueUrl: "http://stone-jade-lake.local:4101/sqs/my-app/jobs",
  WaitTimeSeconds: 20,
  MaxNumberOfMessages: 10,
}));
```

**Redis Backend Implementation:**

```
SQS Queue: my-app/jobs
Redis Keys:
  - sqs:my-app:jobs:messages      (LIST - pending messages)
  - sqs:my-app:jobs:inflight      (HASH - messages being processed)
  - sqs:my-app:jobs:delayed       (ZSET - delayed messages)
  - sqs:my-app:jobs:deadletter    (LIST - failed messages)
  - sqs:my-app:jobs:attributes    (HASH - queue configuration)
```

**Features:**

| Feature | Description |
|---------|-------------|
| Long Polling | BRPOP with timeout |
| Visibility Timeout | Message hidden while processing |
| Dead Letter Queue | Failed messages after N retries |
| Delay Queues | Delayed message delivery |
| FIFO Queues | Ordered, exactly-once (with Redis Streams) |
| Message Attributes | Metadata on messages |

---

### DynamoDB (NoSQL)

**Protocol:** `zen-garden:dynamodb//{app-name}`

**Backends:** MongoDB (recommended), SQLite (lightweight)

**Description:** NoSQL document database with flexible schemas, secondary indexes, and query capabilities.

**API Compatibility:**

| Operation | Supported | Notes |
|-----------|-----------|-------|
| PutItem | ✓ | |
| GetItem | ✓ | |
| UpdateItem | ✓ | |
| DeleteItem | ✓ | |
| Query | ✓ | |
| Scan | ✓ | |
| BatchGetItem | ✓ | |
| BatchWriteItem | ✓ | |
| CreateTable | ✓ | |
| DeleteTable | ✓ | |
| DescribeTable | ✓ | |
| ListTables | ✓ | |
| UpdateTable | ✓ | GSI management |
| TransactWriteItems | ✓ | MongoDB transactions |
| TransactGetItems | ✓ | |

**Connection:**

```javascript
const dynamo = new DynamoDBClient({
  endpoint: await zenGarden.resolve("zen-garden:dynamodb//my-app"),
  credentials: { accessKeyId: "zen-garden", secretAccessKey: "zen-garden" },
  region: "zen-garden",
});

// Put item
await dynamo.send(new PutItemCommand({
  TableName: "users",
  Item: {
    userId: { S: "user-123" },
    name: { S: "Alice" },
    email: { S: "alice@example.com" },
  },
}));

// Query
const response = await dynamo.send(new QueryCommand({
  TableName: "users",
  KeyConditionExpression: "userId = :uid",
  ExpressionAttributeValues: {
    ":uid": { S: "user-123" },
  },
}));
```

**MongoDB Backend Mapping:**

```
DynamoDB Table: users
MongoDB Collection: dynamodb_myapp_users

DynamoDB Item:
{
  "userId": { "S": "user-123" },
  "name": { "S": "Alice" },
  "count": { "N": "42" }
}

MongoDB Document:
{
  "_id": "user-123",
  "_pk": "user-123",
  "_sk": null,
  "userId": "user-123",
  "name": "Alice",
  "count": 42,
  "_raw": { ... }  // Original DynamoDB format for fidelity
}
```

**SQLite Backend (Lightweight):**

For resource-constrained environments:

```sql
CREATE TABLE dynamodb_items (
  app TEXT NOT NULL,
  table_name TEXT NOT NULL,
  pk TEXT NOT NULL,
  sk TEXT,
  data JSON NOT NULL,
  gsi1pk TEXT,
  gsi1sk TEXT,
  gsi2pk TEXT,
  gsi2sk TEXT,
  updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (app, table_name, pk, sk)
);

CREATE INDEX idx_gsi1 ON dynamodb_items (app, table_name, gsi1pk, gsi1sk);
CREATE INDEX idx_gsi2 ON dynamodb_items (app, table_name, gsi2pk, gsi2sk);
```

---

### Secrets Manager

**Protocol:** `zen-garden:secrets//{app-name}`

**Backend:** Keystone (Pond)

**Requires:** Pond 🔒

**Description:** Secure storage for sensitive data like database passwords, API keys, and certificates.

**API Compatibility:**

| Operation | Supported | Notes |
|-----------|-----------|-------|
| CreateSecret | ✓ | |
| GetSecretValue | ✓ | |
| PutSecretValue | ✓ | |
| UpdateSecret | ✓ | |
| DeleteSecret | ✓ | |
| ListSecrets | ✓ | |
| DescribeSecret | ✓ | |
| RotateSecret | ✓ | |
| TagResource | ✓ | |
| UntagResource | ✓ | |

**Connection:**

```javascript
const secrets = new SecretsManagerClient({
  endpoint: await zenGarden.resolve("zen-garden:secrets//my-app"),
  credentials: { accessKeyId: "zen-garden", secretAccessKey: "zen-garden" },
  region: "zen-garden",
});

// Get secret
const response = await secrets.send(new GetSecretValueCommand({
  SecretId: "database/password",
}));

const password = response.SecretString;
```

**Storage:**

Secrets are stored in Keystone, encrypted at rest:

```
Keystone Storage:
  secrets/{app-name}/{secret-id}
    → Encrypted with garden key
    → Versioned (previous versions retained)
    → Audit logged
```

**Without Pond:**

```json
{
  "error": "PondRequired",
  "message": "Secrets Manager requires Pond security layer",
  "hint": "Enable Pond with: garden-rake pond"
}
```

---

### Lambda (Functions)

**Protocol:** `zen-garden:lambda//{app-name}`

**Backend:** Container Runtime (Docker/Podman)

**Description:** Serverless function execution triggered by events or HTTP requests.

**API Compatibility:**

| Operation | Supported | Notes |
|-----------|-----------|-------|
| Invoke | ✓ | Sync and async |
| CreateFunction | ✓ | |
| UpdateFunctionCode | ✓ | |
| UpdateFunctionConfiguration | ✓ | |
| DeleteFunction | ✓ | |
| GetFunction | ✓ | |
| ListFunctions | ✓ | |
| PublishVersion | ✓ | |
| CreateAlias | ✓ | |
| GetFunctionConfiguration | ✓ | |

**Supported Runtimes:**

| Runtime | Image |
|---------|-------|
| nodejs18.x | `ghcr.io/zen-garden/lambda-nodejs:18` |
| nodejs20.x | `ghcr.io/zen-garden/lambda-nodejs:20` |
| python3.9 | `ghcr.io/zen-garden/lambda-python:3.9` |
| python3.11 | `ghcr.io/zen-garden/lambda-python:3.11` |
| python3.12 | `ghcr.io/zen-garden/lambda-python:3.12` |
| java17 | `ghcr.io/zen-garden/lambda-java:17` |
| java21 | `ghcr.io/zen-garden/lambda-java:21` |
| dotnet6 | `ghcr.io/zen-garden/lambda-dotnet:6` |
| dotnet8 | `ghcr.io/zen-garden/lambda-dotnet:8` |
| go1.x | `ghcr.io/zen-garden/lambda-go:1` |
| rust | `ghcr.io/zen-garden/lambda-rust:latest` |
| provided.al2023 | `ghcr.io/zen-garden/lambda-custom:al2023` |

**Deployment:**

```bash
# Via CLI
garden-rake lambda deploy my-func \
  --runtime nodejs20.x \
  --handler index.handler \
  --zip ./function.zip

# Or via AWS CLI
aws --endpoint-url http://stone.local:4104 lambda create-function \
  --function-name my-func \
  --runtime nodejs20.x \
  --handler index.handler \
  --zip-file fileb://function.zip \
  --role arn:aws:iam::000000000000:role/lambda-role
```

**Invocation:**

```javascript
const lambda = new LambdaClient({
  endpoint: await zenGarden.resolve("zen-garden:lambda//my-app"),
  credentials: { accessKeyId: "zen-garden", secretAccessKey: "zen-garden" },
  region: "zen-garden",
});

const response = await lambda.send(new InvokeCommand({
  FunctionName: "my-func",
  Payload: JSON.stringify({ key: "value" }),
}));

const result = JSON.parse(Buffer.from(response.Payload).toString());
```

**Event Sources:**

| Source | Description |
|--------|-------------|
| S3 | Trigger on object events |
| SQS | Process queue messages |
| SNS | Subscribe to topics |
| Schedule | Cron-based execution |
| HTTP | API Gateway integration |

**Event Source Mapping:**

```yaml
# function.yaml
name: image-processor
runtime: python3.11
handler: handler.process
memory: 512
timeout: 30

triggers:
  - type: s3
    bucket: zen-garden:s3//my-app
    events: ["s3:ObjectCreated:*"]
    prefix: "uploads/"
    suffix: ".jpg"
  
  - type: sqs
    queue: zen-garden:sqs//my-app/image-jobs
    batchSize: 10
  
  - type: schedule
    cron: "0 * * * *"
    input: { "action": "cleanup" }
```

**Execution Flow:**

```
┌─────────────────────────────────────────────────────────────────┐
│                    LAMBDA EXECUTION FLOW                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   1. Invocation request arrives                                 │
│                                                                 │
│   2. Bridge checks warm container pool:                         │
│      → Warm container available? Use it                         │
│      → No warm container? Cold start                            │
│                                                                 │
│   3. Cold start (if needed):                                    │
│      a. Find best stone for execution:                          │
│         - GPU function → stone with GPU                         │
│         - High memory → stone with RAM                          │
│         - Any → least loaded stone                              │
│      b. Pull runtime image (cached)                             │
│      c. Fetch function code from S3                             │
│      d. Start container                                         │
│      e. Initialize runtime                                      │
│                                                                 │
│   4. Execute handler with event payload                         │
│                                                                 │
│   5. Return response                                            │
│                                                                 │
│   6. Keep container warm (configurable timeout)                 │
│      Default: 5 minutes                                         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Function Storage:**

```
zen-garden:s3//zg-aws-bridge/lambda/
├── functions/
│   ├── my-app/
│   │   ├── my-func/
│   │   │   ├── code.zip
│   │   │   ├── config.yaml
│   │   │   └── versions/
│   │   │       ├── 1/
│   │   │       └── 2/
```

---

### SNS (Pub/Sub)

**Protocol:** `zen-garden:sns//{app-name}`

**Backends:** Redis (Pub/Sub), Internal Event Bus

**Description:** Publish/subscribe messaging for fan-out, notifications, and event distribution.

**API Compatibility:**

| Operation | Supported | Notes |
|-----------|-----------|-------|
| CreateTopic | ✓ | |
| DeleteTopic | ✓ | |
| ListTopics | ✓ | |
| Publish | ✓ | |
| Subscribe | ✓ | SQS, Lambda, HTTP |
| Unsubscribe | ✓ | |
| ListSubscriptions | ✓ | |
| ListSubscriptionsByTopic | ✓ | |
| GetTopicAttributes | ✓ | |
| SetTopicAttributes | ✓ | |
| ConfirmSubscription | ✓ | For HTTP |

**Connection:**

```javascript
const sns = new SNSClient({
  endpoint: await zenGarden.resolve("zen-garden:sns//my-app"),
  credentials: { accessKeyId: "zen-garden", secretAccessKey: "zen-garden" },
  region: "zen-garden",
});

// Create topic
await sns.send(new CreateTopicCommand({
  Name: "user-events",
}));

// Subscribe SQS queue
await sns.send(new SubscribeCommand({
  TopicArn: "arn:aws:sns:zen-garden:000000000000:my-app-user-events",
  Protocol: "sqs",
  Endpoint: "arn:aws:sqs:zen-garden:000000000000:my-app-notifications",
}));

// Publish message
await sns.send(new PublishCommand({
  TopicArn: "arn:aws:sns:zen-garden:000000000000:my-app-user-events",
  Message: JSON.stringify({ event: "user.created", userId: 123 }),
});
```

**Subscription Types:**

| Protocol | Endpoint | Description |
|----------|----------|-------------|
| sqs | Queue ARN | Push to SQS queue |
| lambda | Function ARN | Invoke Lambda function |
| http/https | URL | POST to HTTP endpoint |
| email | Address | Email notification (via SES) |

**Fan-Out Pattern:**

```
Publisher → SNS Topic → ┬→ SQS Queue 1 → Worker A
                        ├→ SQS Queue 2 → Worker B
                        ├→ Lambda Function
                        └→ HTTP Webhook
```

---

### SES (Email)

**Protocol:** `zen-garden:ses//{app-name}`

**Backends:** Mailpit (local dev), SMTP (production)

**Description:** Email sending for transactional email, notifications, and reports.

**API Compatibility:**

| Operation | Supported | Notes |
|-----------|-----------|-------|
| SendEmail | ✓ | |
| SendRawEmail | ✓ | |
| SendTemplatedEmail | ✓ | |
| CreateTemplate | ✓ | |
| DeleteTemplate | ✓ | |
| GetTemplate | ✓ | |
| ListTemplates | ✓ | |
| VerifyEmailIdentity | ✓ | No-op locally |
| ListIdentities | ✓ | |

**Connection:**

```javascript
const ses = new SESClient({
  endpoint: await zenGarden.resolve("zen-garden:ses//my-app"),
  credentials: { accessKeyId: "zen-garden", secretAccessKey: "zen-garden" },
  region: "zen-garden",
});

await ses.send(new SendEmailCommand({
  Source: "noreply@myapp.local",
  Destination: {
    ToAddresses: ["user@example.com"],
  },
  Message: {
    Subject: { Data: "Welcome!" },
    Body: {
      Text: { Data: "Thanks for signing up." },
      Html: { Data: "<h1>Welcome!</h1><p>Thanks for signing up.</p>" },
    },
  },
}));
```

**Mailpit Backend:**

For local development, emails are captured by Mailpit:

- All emails intercepted (never sent externally)
- Web UI to view emails: `http://stone.local:8025`
- API to query emails programmatically
- Perfect for testing email flows

**SMTP Backend:**

For production, forward to real SMTP:

```yaml
# Bridge config
services:
  ses:
    backend: smtp
    smtp:
      host: smtp.sendgrid.net
      port: 587
      username: apikey
      password: ${SENDGRID_API_KEY}
      tls: true
```

---

### CloudWatch Logs

**Protocol:** `zen-garden:logs//{app-name}`

**Backends:** Loki (recommended), File-based

**Description:** Centralized logging across all services and functions.

**API Compatibility:**

| Operation | Supported | Notes |
|-----------|-----------|-------|
| CreateLogGroup | ✓ | |
| DeleteLogGroup | ✓ | |
| CreateLogStream | ✓ | |
| DeleteLogStream | ✓ | |
| PutLogEvents | ✓ | |
| GetLogEvents | ✓ | |
| FilterLogEvents | ✓ | |
| DescribeLogGroups | ✓ | |
| DescribeLogStreams | ✓ | |
| PutRetentionPolicy | ✓ | |

**Connection:**

```javascript
const logs = new CloudWatchLogsClient({
  endpoint: await zenGarden.resolve("zen-garden:logs//my-app"),
  credentials: { accessKeyId: "zen-garden", secretAccessKey: "zen-garden" },
  region: "zen-garden",
});

// Put log events
await logs.send(new PutLogEventsCommand({
  logGroupName: "/my-app/api",
  logStreamName: "instance-1",
  logEvents: [
    { timestamp: Date.now(), message: "Request received: GET /users" },
    { timestamp: Date.now(), message: "Response sent: 200 OK" },
  ],
}));

// Query logs
const response = await logs.send(new FilterLogEventsCommand({
  logGroupName: "/my-app/api",
  filterPattern: "ERROR",
  startTime: Date.now() - 3600000, // Last hour
}));
```

**Loki Backend:**

Logs stored in Loki for efficient querying:

```
CloudWatch Log Group: /my-app/api
Loki Labels: {app="my-app", group="api", stream="instance-1"}
```

Query via Loki's LogQL or CloudWatch Logs API.

**File Backend (Lightweight):**

For minimal setups:

```
/var/zg-aws-bridge/logs/
├── my-app/
│   └── api/
│       ├── instance-1.log
│       └── instance-2.log
```

---

### Parameter Store (SSM)

**Protocol:** `zen-garden:ssm//{app-name}`

**Backend:** Garden Configuration

**Description:** Configuration management for non-sensitive settings, feature flags, and environment config.

**API Compatibility:**

| Operation | Supported | Notes |
|-----------|-----------|-------|
| GetParameter | ✓ | |
| GetParameters | ✓ | |
| GetParametersByPath | ✓ | |
| PutParameter | ✓ | |
| DeleteParameter | ✓ | |
| DeleteParameters | ✓ | |
| DescribeParameters | ✓ | |
| GetParameterHistory | ✓ | |
| LabelParameterVersion | ✓ | |

**Connection:**

```javascript
const ssm = new SSMClient({
  endpoint: await zenGarden.resolve("zen-garden:ssm//my-app"),
  credentials: { accessKeyId: "zen-garden", secretAccessKey: "zen-garden" },
  region: "zen-garden",
});

// Get parameter
const response = await ssm.send(new GetParameterCommand({
  Name: "/my-app/config/api-url",
}));

const apiUrl = response.Parameter.Value;

// Get multiple parameters by path
const params = await ssm.send(new GetParametersByPathCommand({
  Path: "/my-app/feature-flags/",
  Recursive: true,
}));
```

**Storage:**

```yaml
# Garden parameter store
parameters:
  my-app:
    config:
      api-url: "https://api.example.com"
      timeout: "30"
    feature-flags:
      new-ui: "true"
      dark-mode: "false"
```

**Parameter Types:**

| Type | Description | For Secrets? |
|------|-------------|--------------|
| String | Plain text | No |
| StringList | Comma-separated values | No |
| SecureString | Encrypted | Use Secrets Manager instead |

---

### KMS (Key Management)

**Protocol:** `zen-garden:kms//{app-name}`

**Backend:** Keystone (Pond)

**Requires:** Pond 🔒

**Description:** Cryptographic key management for encryption, signing, and key derivation.

**API Compatibility:**

| Operation | Supported | Notes |
|-----------|-----------|-------|
| CreateKey | ✓ | |
| DescribeKey | ✓ | |
| ListKeys | ✓ | |
| EnableKey | ✓ | |
| DisableKey | ✓ | |
| ScheduleKeyDeletion | ✓ | |
| Encrypt | ✓ | |
| Decrypt | ✓ | |
| GenerateDataKey | ✓ | |
| GenerateDataKeyWithoutPlaintext | ✓ | |
| Sign | ✓ | |
| Verify | ✓ | |
| CreateAlias | ✓ | |
| DeleteAlias | ✓ | |

**Connection:**

```javascript
const kms = new KMSClient({
  endpoint: await zenGarden.resolve("zen-garden:kms//my-app"),
  credentials: { accessKeyId: "zen-garden", secretAccessKey: "zen-garden" },
  region: "zen-garden",
});

// Encrypt data
const encrypted = await kms.send(new EncryptCommand({
  KeyId: "alias/my-app/data-key",
  Plaintext: Buffer.from("sensitive data"),
}));

// Decrypt data
const decrypted = await kms.send(new DecryptCommand({
  KeyId: "alias/my-app/data-key",
  CiphertextBlob: encrypted.CiphertextBlob,
}));

// Generate data key for client-side encryption
const dataKey = await kms.send(new GenerateDataKeyCommand({
  KeyId: "alias/my-app/data-key",
  KeySpec: "AES_256",
}));
// Use dataKey.Plaintext to encrypt locally
// Store dataKey.CiphertextBlob with encrypted data
```

**Key Types:**

| Type | Use Case |
|------|----------|
| SYMMETRIC_DEFAULT | Encrypt/decrypt data |
| RSA_2048/4096 | Sign/verify, encrypt |
| ECC_NIST_P256/384/521 | Sign/verify |
| ECC_SECG_P256K1 | Sign/verify (Bitcoin compatible) |

---

## Backend Adapters

### Adapter Interface

All backends implement a common interface:

```rust
#[async_trait]
trait BackendAdapter: Send + Sync {
    /// Check if backend is healthy
    async fn health_check(&self) -> Result<HealthStatus>;
    
    /// Get backend statistics
    async fn stats(&self) -> Result<BackendStats>;
    
    /// Graceful shutdown
    async fn shutdown(&self) -> Result<()>;
}
```

### SQS Adapters

#### Redis Adapter

```rust
struct RedisSqsAdapter {
    redis: RedisClient,
    app: String,
}

#[async_trait]
impl SqsAdapter for RedisSqsAdapter {
    async fn send_message(&self, queue: &str, message: &SqsMessage) -> Result<String> {
        let key = format!("sqs:{}:{}:messages", self.app, queue);
        let message_json = serde_json::to_string(message)?;
        
        self.redis.lpush(&key, &message_json).await?;
        
        Ok(message.message_id.clone())
    }
    
    async fn receive_messages(
        &self,
        queue: &str,
        max_messages: u32,
        wait_time: Duration,
    ) -> Result<Vec<SqsMessage>> {
        let key = format!("sqs:{}:{}:messages", self.app, queue);
        let inflight_key = format!("sqs:{}:{}:inflight", self.app, queue);
        
        let mut messages = Vec::new();
        
        for _ in 0..max_messages {
            // BRPOPLPUSH atomically moves message to inflight
            let result = self.redis
                .brpoplpush(&key, &inflight_key, wait_time.as_secs())
                .await?;
            
            match result {
                Some(json) => {
                    let mut message: SqsMessage = serde_json::from_str(&json)?;
                    message.receipt_handle = Some(generate_receipt_handle());
                    messages.push(message);
                }
                None => break, // Timeout, no more messages
            }
        }
        
        Ok(messages)
    }
    
    async fn delete_message(&self, queue: &str, receipt_handle: &str) -> Result<()> {
        let inflight_key = format!("sqs:{}:{}:inflight", self.app, queue);
        
        // Remove from inflight
        self.redis.lrem(&inflight_key, 1, receipt_handle).await?;
        
        Ok(())
    }
}
```

#### RabbitMQ Adapter

```rust
struct RabbitMqSqsAdapter {
    connection: RabbitMqConnection,
    app: String,
}

#[async_trait]
impl SqsAdapter for RabbitMqSqsAdapter {
    async fn send_message(&self, queue: &str, message: &SqsMessage) -> Result<String> {
        let channel = self.connection.create_channel().await?;
        let queue_name = format!("sqs.{}.{}", self.app, queue);
        
        channel.basic_publish(
            "",
            &queue_name,
            BasicPublishOptions::default(),
            message.body.as_bytes(),
            BasicProperties::default()
                .with_message_id(message.message_id.clone()),
        ).await?;
        
        Ok(message.message_id.clone())
    }
    
    // ... similar implementations
}
```

### DynamoDB Adapters

#### MongoDB Adapter

```rust
struct MongoDynamoAdapter {
    client: MongoClient,
    db: Database,
    app: String,
    table_schemas: HashMap<String, TableSchema>,
}

#[async_trait]
impl DynamoAdapter for MongoDynamoAdapter {
    async fn put_item(&self, table: &str, item: &DynamoItem) -> Result<()> {
        let collection = self.db.collection::<Document>(
            &format!("dynamodb_{}_{}", self.app, table)
        );
        
        let schema = self.table_schemas.get(table)
            .ok_or_else(|| Error::TableNotFound(table.to_string()))?;
        
        // Extract key
        let pk = extract_attribute(item, &schema.partition_key)?;
        let sk = schema.sort_key.as_ref()
            .map(|sk| extract_attribute(item, sk))
            .transpose()?;
        
        // Convert to MongoDB document
        let doc = dynamo_item_to_bson(item)?;
        
        // Upsert
        let filter = match sk {
            Some(sk) => doc! { "_pk": pk, "_sk": sk },
            None => doc! { "_pk": pk },
        };
        
        collection.replace_one(
            filter,
            doc,
            ReplaceOptions::builder().upsert(true).build(),
        ).await?;
        
        Ok(())
    }
    
    async fn query(
        &self,
        table: &str,
        key_condition: &KeyConditionExpression,
        filter: Option<&FilterExpression>,
    ) -> Result<Vec<DynamoItem>> {
        let collection = self.db.collection::<Document>(
            &format!("dynamodb_{}_{}", self.app, table)
        );
        
        // Convert DynamoDB expressions to MongoDB query
        let mongo_filter = build_mongo_query(key_condition, filter)?;
        
        let cursor = collection.find(mongo_filter, None).await?;
        let docs: Vec<Document> = cursor.try_collect().await?;
        
        docs.into_iter()
            .map(bson_to_dynamo_item)
            .collect()
    }
}
```

#### SQLite Adapter

```rust
struct SqliteDynamoAdapter {
    pool: SqlitePool,
    app: String,
}

#[async_trait]
impl DynamoAdapter for SqliteDynamoAdapter {
    async fn put_item(&self, table: &str, item: &DynamoItem) -> Result<()> {
        let schema = self.get_table_schema(table).await?;
        
        let pk = extract_string_attribute(item, &schema.partition_key)?;
        let sk = schema.sort_key.as_ref()
            .map(|sk| extract_string_attribute(item, sk))
            .transpose()?;
        
        let json = serde_json::to_string(item)?;
        
        sqlx::query(r#"
            INSERT INTO dynamodb_items (app, table_name, pk, sk, data, updated_at)
            VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT (app, table_name, pk, sk) DO UPDATE SET
                data = excluded.data,
                updated_at = CURRENT_TIMESTAMP
        "#)
        .bind(&self.app)
        .bind(table)
        .bind(&pk)
        .bind(&sk)
        .bind(&json)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
```

---

## Auto-Provisioning

### Concept

When a user enables a service that requires a backend, the bridge can automatically provision the required offering.

### Flow

```
┌─────────────────────────────────────────────────────────────────┐
│              AUTO-PROVISIONING FLOW                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   1. User enables SQS in UI                                     │
│                                                                 │
│   2. Bridge checks: "Do I have a queue backend?"                │
│      → Query garden: "Who has redis?"                           │
│      → Not found                                                │
│                                                                 │
│   3. Auto-provision enabled?                                    │
│      → Yes: Continue                                            │
│      → No: Show "Backend required" message                      │
│                                                                 │
│   4. Select backend based on preferences:                       │
│      → User preference: redis                                   │
│      → Or default: redis (for SQS)                              │
│                                                                 │
│   5. Provision offering:                                        │
│      POST /api/v1/offerings                                     │
│      {                                                          │
│        "type": "redis",                                         │
│        "managed_by": "zg-aws-bridge"                            │
│      }                                                          │
│                                                                 │
│   6. Wait for offering to be healthy                            │
│                                                                 │
│   7. Connect and configure adapter                              │
│                                                                 │
│   8. Announce protocol handler                                  │
│                                                                 │
│   9. Service ready                                              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Provisioning API

The bridge uses the Moss API to provision offerings:

```http
POST /api/v1/offerings
Authorization: ZenGarden offering=zg-aws-bridge

{
  "type": "redis",
  "name": "redis-sqs",
  "managed_by": "zg-aws-bridge",
  "config": {
    "maxmemory": "256mb",
    "maxmemory-policy": "allkeys-lru"
  }
}
```

### Backend Selection

```rust
fn select_backend(service: &Service, available: &[Offering]) -> BackendChoice {
    // Check user preference
    if let Some(preferred) = service.config.preferred_backend {
        if available.iter().any(|o| o.offering_type == preferred) {
            return BackendChoice::Use(preferred);
        }
    }
    
    // Check available offerings
    for backend in service.supported_backends() {
        if available.iter().any(|o| o.offering_type == backend) {
            return BackendChoice::Use(backend);
        }
    }
    
    // Need to provision
    let default = service.default_backend();
    BackendChoice::Provision(default)
}
```

### Default Backends

| Service | Default Backend | Alternatives |
|---------|----------------|--------------|
| SQS | Redis | RabbitMQ, Built-in |
| SNS | Redis | Built-in |
| DynamoDB | MongoDB | SQLite |
| Logs | Loki | File-based |
| SES | Mailpit | SMTP |
| Lambda | Container Runtime | — |

---

## Managed Offerings

### Concept

Offerings provisioned by the bridge are **managed offerings**. The bridge owns their lifecycle.

### Managed-By Relationship

```yaml
# Offering state
offering_id: redis-abc123
offering_name: redis-sqs
offering_type: redis
managed_by: zg-aws-bridge    # ← Bridge owns this

# Metadata
managed_by_metadata:
  service: sqs
  provisioned_at: 2026-01-23T10:00:00Z
  auto_provisioned: true
```

### Lifecycle Rules

| Action | Managed Offering Behavior |
|--------|--------------------------|
| Bridge enables service | Provision if missing |
| Bridge disables service | Option to release |
| Bridge removed | Prompt about managed offerings |
| User adopts offering | Remove managed_by, user owns it |
| Garden hydrates | Restore with managed_by relationship |

### Adoption

Users can take ownership of managed offerings:

```bash
# List managed offerings
garden-rake offerings --managed-by zg-aws-bridge

MANAGED BY: zg-aws-bridge
───────────────────────────────────────────────────
  redis-sqs     redis     SQS backend     stone-silver-stream
  mongodb-ddb   mongodb   DynamoDB backend stone-jade-lake
  mailpit       mailpit   SES backend     stone-morning-mist

# Adopt an offering (take ownership)
garden-rake adopt redis-sqs

Removing managed_by relationship...
redis-sqs is now owned by you.

# Or via API
POST /api/v1/offerings/redis-abc123/adopt
```

### Removal Flow

When the bridge is removed:

```bash
$ garden-rake release zg-aws-bridge

ZG-AWS-BRIDGE REMOVAL
───────────────────────────────────────────────────
  This will disable all AWS-compatible services.
  
  Managed offerings:
    redis-sqs     (SQS backend)      234 messages
    mongodb-ddb   (DynamoDB backend) 12,847 items
    mailpit       (SES backend)      1,203 emails
  
  What should happen to managed offerings?
  
    [1] Remove all (delete data)
    [2] Keep all (adopt them)
    [3] Choose individually
    [4] Cancel
    
Choice [4]: 3

  redis-sqs: [K]eep / [R]emove? k
  mongodb-ddb: [K]eep / [R]emove? k
  mailpit: [K]eep / [R]emove? r

Adopting redis-sqs...                              ✓
Adopting mongodb-ddb...                            ✓
Removing mailpit...                                ✓
Removing zg-aws-bridge...                          ✓

Done. Kept offerings are now owned by you.
```

---

## Bridge UI

### Main Dashboard

```
┌─────────────────────────────────────────────────────────────────┐
│  🌿 Zen Garden AWS Bridge                    stone-jade-lake    │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  AWS Services                                        [Refresh]  │
│  ──────────────────────────────────────────────────────────────│
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ ✓  S3 Storage                                  [Config] │   │
│  │    Backend: seed-glorious-dawn                          │   │
│  │    Status: ● Healthy    Objects: 1,234    Size: 4.2 GB  │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ ✓  SQS Queues                                  [Config] │   │
│  │    Backend: redis (stone-silver-stream)                 │   │
│  │    Status: ● Healthy    Queues: 3    Messages: 847      │   │
│  │    Managed: Yes                                         │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ ✓  DynamoDB Tables                             [Config] │   │
│  │    Backend: mongodb (stone-jade-lake)                   │   │
│  │    Status: ● Healthy    Tables: 5    Items: 12,847      │   │
│  │    Managed: Yes                                         │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ ✓  Secrets Manager                             [Config] │   │
│  │    Backend: Keystone (Pond)                             │   │
│  │    Status: ● Healthy    Secrets: 12                     │   │
│  │    🔒 Pond                                               │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ ○  Lambda Functions                            [Enable] │   │
│  │    Serverless function execution                        │   │
│  │    Requires: Container runtime                          │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ ○  SNS Topics                                  [Enable] │   │
│  │    Pub/sub messaging and fan-out                        │   │
│  │    Will use: Redis (shared with SQS)                    │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ ○  SES Email                                   [Enable] │   │
│  │    Local email testing and capture                      │   │
│  │    Will provision: Mailpit                              │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ ○  CloudWatch Logs                             [Enable] │   │
│  │    Centralized logging                                  │   │
│  │    Will provision: Loki                                 │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ ○  Parameter Store                             [Enable] │   │
│  │    Configuration management                             │   │
│  │    Built-in (no backend needed)                         │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ ○  KMS                                         [Enable] │   │
│  │    Key management and encryption                        │   │
│  │    🔒 Requires Pond                                      │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ──────────────────────────────────────────────────────────────│
│                                                                 │
│  Backups                                                        │
│  ──────────────────────────────────────────────────────────────│
│  Last cultivation: 2 hours ago                                  │
│  Seed bank: seed-glorious-dawn (28 GB free)                     │
│  All backends: ✓ Cultivated                                     │
│                                                                 │
│  ──────────────────────────────────────────────────────────────│
│                                                                 │
│  Connection Info                                                │
│  ──────────────────────────────────────────────────────────────│
│  S3:       http://stone-jade-lake.local:4100                    │
│  SQS:      http://stone-jade-lake.local:4101                    │
│  DynamoDB: http://stone-jade-lake.local:4102                    │
│  Secrets:  http://stone-jade-lake.local:4103                    │
│                                                                 │
│  Credentials: zen-garden / zen-garden                           │
│  Region: zen-garden                                             │
│                                                                 │
│  [Copy .env]  [Copy SDK Config]                                 │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Enable Service Dialog

```
┌─────────────────────────────────────────────────────────────────┐
│  Enable SQS                                              [X]    │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  SQS provides message queuing for async processing.             │
│                                                                 │
│  Backend                                                        │
│  ──────────────────────────────────────────────────────────────│
│                                                                 │
│  No queue backend found in garden.                              │
│                                                                 │
│  Select backend to provision:                                   │
│                                                                 │
│  ● Redis (recommended)                                          │
│    Fast, simple, great for most use cases                       │
│    Memory: ~50MB base + messages                                │
│                                                                 │
│  ○ RabbitMQ                                                     │
│    Advanced routing, AMQP support                               │
│    Memory: ~150MB base                                          │
│                                                                 │
│  ○ Built-in queue                                               │
│    Lightweight, no external dependency                          │
│    Limited features, not recommended for production             │
│                                                                 │
│  ☑ Auto-provision backend                                       │
│    Bridge will create and manage the Redis offering             │
│                                                                 │
│  ──────────────────────────────────────────────────────────────│
│                                                                 │
│                               [Cancel]              [Enable]    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Service Configuration

```
┌─────────────────────────────────────────────────────────────────┐
│  SQS Configuration                                       [X]    │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Backend                                                        │
│  ──────────────────────────────────────────────────────────────│
│  Current: Redis (stone-silver-stream)                           │
│  Managed: Yes (by this bridge)                                  │
│  Status: ● Healthy                                              │
│                                                                 │
│  [Change Backend]  [Adopt to User]                              │
│                                                                 │
│  Queues                                                         │
│  ──────────────────────────────────────────────────────────────│
│  ┌──────────────────────┬──────────┬──────────┬────────────┐   │
│  │ Queue                │ Messages │ In-flight│ Actions    │   │
│  ├──────────────────────┼──────────┼──────────┼────────────┤   │
│  │ my-app/jobs          │ 234      │ 12       │ [Purge][⋮] │   │
│  │ my-app/notifications │ 0        │ 0        │ [Purge][⋮] │   │
│  │ my-app/dead-letter   │ 47       │ 0        │ [Inspect]  │   │
│  └──────────────────────┴──────────┴──────────┴────────────┘   │
│                                                                 │
│  [Create Queue]                                                 │
│                                                                 │
│  Default Settings                                               │
│  ──────────────────────────────────────────────────────────────│
│  Visibility timeout:    [30        ] seconds                    │
│  Message retention:     [4         ] days                       │
│  Max message size:      [256       ] KB                         │
│  Receive wait time:     [20        ] seconds (long polling)     │
│                                                                 │
│  Dead Letter Queue                                              │
│  ──────────────────────────────────────────────────────────────│
│  ☑ Enable dead letter queue                                     │
│  Max receive count:     [3         ] before DLQ                 │
│                                                                 │
│  ──────────────────────────────────────────────────────────────│
│                                                                 │
│                   [Disable SQS]              [Save Changes]     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Connection Info Export

```
┌─────────────────────────────────────────────────────────────────┐
│  Export Configuration                                    [X]    │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  .env format                                          [Copy]    │
│  ──────────────────────────────────────────────────────────────│
│  AWS_ENDPOINT_URL_S3=http://stone-jade-lake.local:4100          │
│  AWS_ENDPOINT_URL_SQS=http://stone-jade-lake.local:4101         │
│  AWS_ENDPOINT_URL_DYNAMODB=http://stone-jade-lake.local:4102    │
│  AWS_ENDPOINT_URL_SECRETSMANAGER=http://stone-jade-lake.local:4103
│  AWS_ACCESS_KEY_ID=zen-garden                                   │
│  AWS_SECRET_ACCESS_KEY=zen-garden                               │
│  AWS_REGION=zen-garden                                          │
│                                                                 │
│  Zen Garden SDK format                                [Copy]    │
│  ──────────────────────────────────────────────────────────────│
│  ZEN_GARDEN_S3=zen-garden:s3//my-app                            │
│  ZEN_GARDEN_SQS=zen-garden:sqs//my-app                          │
│  ZEN_GARDEN_DYNAMODB=zen-garden:dynamodb//my-app                │
│  ZEN_GARDEN_SECRETS=zen-garden:secrets//my-app                  │
│                                                                 │
│  JavaScript/TypeScript                                [Copy]    │
│  ──────────────────────────────────────────────────────────────│
│  const config = {                                               │
│    endpoint: "http://stone-jade-lake.local:4100",               │
│    credentials: {                                               │
│      accessKeyId: "zen-garden",                                 │
│      secretAccessKey: "zen-garden",                             │
│    },                                                           │
│    region: "zen-garden",                                        │
│    forcePathStyle: true,                                        │
│  };                                                             │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Bridge API

### Base URL

```
http://{stone}.local:4199/api/v1/bridge
```

### Endpoints

#### List Services

```http
GET /api/v1/bridge/services

Response 200:
{
  "services": [
    {
      "name": "s3",
      "display_name": "S3 Storage",
      "enabled": true,
      "port": 4100,
      "protocol": "zen-garden:s3",
      "backend": {
        "type": "seed-bank",
        "name": "seed-glorious-dawn",
        "managed": false,
        "healthy": true
      },
      "stats": {
        "objects": 1234,
        "size_bytes": 4509715200
      },
      "pond_required": false
    },
    {
      "name": "sqs",
      "display_name": "SQS Queues",
      "enabled": true,
      "port": 4101,
      "protocol": "zen-garden:sqs",
      "backend": {
        "type": "redis",
        "offering_id": "redis-abc123",
        "offering_name": "redis-sqs",
        "stone": "stone-silver-stream",
        "managed": true,
        "healthy": true
      },
      "stats": {
        "queues": 3,
        "messages": 847
      },
      "pond_required": false
    },
    {
      "name": "secrets",
      "display_name": "Secrets Manager",
      "enabled": true,
      "port": 4103,
      "protocol": "zen-garden:secrets",
      "backend": {
        "type": "keystone",
        "healthy": true
      },
      "stats": {
        "secrets": 12
      },
      "pond_required": true,
      "pond_enabled": true
    },
    {
      "name": "lambda",
      "display_name": "Lambda Functions",
      "enabled": false,
      "port": 4104,
      "protocol": "zen-garden:lambda",
      "backend": null,
      "requirements": ["container-runtime"],
      "pond_required": false
    }
  ]
}
```

#### Get Service

```http
GET /api/v1/bridge/services/{service}

Response 200:
{
  "name": "sqs",
  "display_name": "SQS Queues",
  "enabled": true,
  "port": 4101,
  "protocol": "zen-garden:sqs",
  "backend": {
    "type": "redis",
    "offering_id": "redis-abc123",
    "offering_name": "redis-sqs",
    "stone": "stone-silver-stream",
    "managed": true,
    "healthy": true
  },
  "config": {
    "default_visibility_timeout_seconds": 30,
    "default_message_retention_days": 4,
    "max_message_size_kb": 256,
    "default_wait_time_seconds": 20,
    "dead_letter_queue_enabled": true,
    "dead_letter_max_receive_count": 3
  },
  "stats": {
    "queues": 3,
    "messages": 847,
    "messages_in_flight": 12
  },
  "queues": [
    {
      "name": "my-app/jobs",
      "messages": 234,
      "in_flight": 12,
      "created": "2026-01-20T10:00:00Z"
    },
    {
      "name": "my-app/notifications",
      "messages": 0,
      "in_flight": 0,
      "created": "2026-01-21T14:30:00Z"
    }
  ]
}
```

#### Enable Service

```http
POST /api/v1/bridge/services/{service}/enable

Request:
{
  "backend": "redis",           // or "auto"
  "auto_provision": true,
  "config": {
    "default_visibility_timeout_seconds": 60
  }
}

Response 200:
{
  "service": "sqs",
  "enabled": true,
  "backend": {
    "type": "redis",
    "offering_id": "redis-abc123",
    "provisioned": true,
    "managed": true
  },
  "endpoint": "http://stone-jade-lake.local:4101"
}

Response 400 (Pond required):
{
  "error": "PondRequired",
  "service": "secrets",
  "message": "Secrets Manager requires Pond security layer",
  "hint": "Enable Pond with: garden-rake pond"
}

Response 400 (Backend unavailable):
{
  "error": "BackendUnavailable",
  "service": "sqs",
  "message": "No queue backend available and auto_provision is false",
  "available_backends": [],
  "suggested_backends": ["redis", "rabbitmq"]
}
```

#### Disable Service

```http
POST /api/v1/bridge/services/{service}/disable

Request:
{
  "remove_backend": false    // Keep managed offering
}

Response 200:
{
  "service": "sqs",
  "enabled": false,
  "backend_kept": true,
  "backend_adopted": true    // Now owned by user
}
```

#### Update Service Config

```http
PUT /api/v1/bridge/services/{service}/config

Request:
{
  "default_visibility_timeout_seconds": 60,
  "dead_letter_queue_enabled": false
}

Response 200:
{
  "service": "sqs",
  "config": {
    "default_visibility_timeout_seconds": 60,
    "default_message_retention_days": 4,
    "max_message_size_kb": 256,
    "default_wait_time_seconds": 20,
    "dead_letter_queue_enabled": false
  }
}
```

#### Migrate Backend

```http
POST /api/v1/bridge/services/{service}/migrate

Request:
{
  "to_backend": "rabbitmq",
  "auto_provision": true,
  "migrate_data": true       // Attempt to migrate existing data
}

Response 200:
{
  "service": "sqs",
  "migration": {
    "from": "redis",
    "to": "rabbitmq",
    "status": "completed",
    "queues_migrated": 3,
    "messages_migrated": 847
  },
  "new_backend": {
    "type": "rabbitmq",
    "offering_id": "rabbitmq-def456",
    "managed": true
  }
}
```

#### Get Connection Info

```http
GET /api/v1/bridge/connection-info

Query params:
  - format: env | sdk | json | javascript | python | go
  - app: application name (for SDK format)

Response 200 (format=env):
AWS_ENDPOINT_URL_S3=http://stone-jade-lake.local:4100
AWS_ENDPOINT_URL_SQS=http://stone-jade-lake.local:4101
AWS_ENDPOINT_URL_DYNAMODB=http://stone-jade-lake.local:4102
AWS_ACCESS_KEY_ID=zen-garden
AWS_SECRET_ACCESS_KEY=zen-garden
AWS_REGION=zen-garden

Response 200 (format=json):
{
  "endpoints": {
    "s3": "http://stone-jade-lake.local:4100",
    "sqs": "http://stone-jade-lake.local:4101",
    "dynamodb": "http://stone-jade-lake.local:4102",
    "secrets": "http://stone-jade-lake.local:4103"
  },
  "credentials": {
    "access_key_id": "zen-garden",
    "secret_access_key": "zen-garden"
  },
  "region": "zen-garden"
}
```

#### Health Check

```http
GET /api/v1/bridge/health

Response 200:
{
  "status": "healthy",
  "services": {
    "s3": "healthy",
    "sqs": "healthy",
    "dynamodb": "healthy",
    "secrets": "healthy"
  },
  "backends": {
    "seed-glorious-dawn": "healthy",
    "redis-sqs": "healthy",
    "mongodb-ddb": "healthy",
    "keystone": "healthy"
  }
}
```

---

## CLI Integration

### Commands

```bash
# Deploy the bridge
garden-rake offer zg-aws-bridge

# List bridge services
garden-rake bridge services

AWS BRIDGE SERVICES
───────────────────────────────────────────────────
  ✓ s3        seed-glorious-dawn            Healthy
  ✓ sqs       redis (managed)               Healthy    847 msgs
  ✓ dynamodb  mongodb (managed)             Healthy    12,847 items
  ✓ secrets   keystone                      Healthy    12 secrets
  ○ lambda    (disabled)
  ○ sns       (disabled)
  ○ ses       (disabled)
  ○ logs      (disabled)
  ○ ssm       (disabled)
  ○ kms       (disabled)                    🔒 Pond

# Enable a service
garden-rake bridge enable sqs
garden-rake bridge enable sqs --backend redis
garden-rake bridge enable sqs --backend redis --no-auto-provision

# Disable a service
garden-rake bridge disable sqs
garden-rake bridge disable sqs --keep-backend
garden-rake bridge disable sqs --remove-backend

# Show service details
garden-rake bridge service sqs

SQS SERVICE
───────────────────────────────────────────────────
  Status:    Enabled
  Backend:   Redis (stone-silver-stream)
  Managed:   Yes
  Endpoint:  http://stone-jade-lake.local:4101
  Protocol:  zen-garden:sqs
  
  Configuration:
    Visibility timeout:  30 seconds
    Message retention:   4 days
    Max message size:    256 KB
    Long polling:        20 seconds
    Dead letter queue:   Enabled (after 3 attempts)
  
  Queues:
    my-app/jobs          234 messages (12 in-flight)
    my-app/notifications 0 messages
    my-app/dead-letter   47 messages

# Configure service
garden-rake bridge config sqs --visibility-timeout 60
garden-rake bridge config sqs --set default_visibility_timeout_seconds=60

# Migrate backend
garden-rake bridge migrate sqs --to rabbitmq

# Get connection info
garden-rake bridge connection-info
garden-rake bridge connection-info --format env
garden-rake bridge connection-info --format env > .env

# Open UI
garden-rake bridge ui
# → Opens http://stone-jade-lake.local:4199 in browser

# Check health
garden-rake bridge health
```

### Lambda Commands

```bash
# Deploy function
garden-rake lambda deploy my-func \
  --runtime nodejs20.x \
  --handler index.handler \
  --zip ./function.zip \
  --memory 256 \
  --timeout 30

# List functions
garden-rake lambda list

LAMBDA FUNCTIONS
───────────────────────────────────────────────────
  my-func         nodejs20.x    256MB    30s    v3
  image-processor python3.11   1024MB   120s    v1
  webhook-handler go1.x         128MB    10s    v5

# Invoke function
garden-rake lambda invoke my-func --payload '{"key": "value"}'
garden-rake lambda invoke my-func --payload-file event.json

# View logs
garden-rake lambda logs my-func
garden-rake lambda logs my-func --follow

# Update function
garden-rake lambda update my-func --zip ./new-function.zip
garden-rake lambda config my-func --memory 512 --timeout 60

# Delete function
garden-rake lambda delete my-func
```

---

## Backup and Recovery

### What Gets Backed Up

Everything is an offering, everything is cultivated:

```
┌─────────────────────────────────────────────────────────────────┐
│                    BACKUP HIERARCHY                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   zg-aws-bridge                                                 │
│       │                                                         │
│       ├── Bridge state (state.yaml)                             │
│       │   → Which services enabled                              │
│       │   → Service configurations                              │
│       │   → Managed offering relationships                      │
│       │                                                         │
│       ├── SQS → Redis offering                                  │
│       │         → All queues and messages                       │
│       │         → Cultivated to seed bank                       │
│       │                                                         │
│       ├── DynamoDB → MongoDB offering                           │
│       │              → All tables and items                     │
│       │              → Cultivated to seed bank                  │
│       │                                                         │
│       ├── Lambda → S3 storage                                   │
│       │           → Function code and config                    │
│       │           → Already in seed bank                        │
│       │                                                         │
│       ├── SES → Mailpit offering                                │
│       │         → Captured emails                               │
│       │         → Cultivated to seed bank                       │
│       │                                                         │
│       ├── Logs → Loki offering                                  │
│       │          → Log data                                     │
│       │          → Cultivated to seed bank                      │
│       │                                                         │
│       └── Secrets → Keystone                                    │
│                     → Encrypted secrets                         │
│                     → Part of Pond backup                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Hydration Recovery

```bash
$ garden-rake hydrate --from seed-glorious-dawn

HYDRATING GARDEN
───────────────────────────────────────────────────
  Seed bank: seed-glorious-dawn
  
  Offerings to restore:
    zg-aws-bridge   AWS Bridge              
    redis-sqs       Redis (managed by zg-aws-bridge)
    mongodb-ddb     MongoDB (managed by zg-aws-bridge)
    mailpit         Mailpit (managed by zg-aws-bridge)
    loki            Loki (managed by zg-aws-bridge)
    
Proceed? [Y/n]: y

Offering zg-aws-bridge wishfully...
  → stone-jade-lake claimed
  → restoring state... ✓

Offering redis-sqs wishfully...
  → stone-silver-stream claimed (managed by zg-aws-bridge)
  → restoring data... ✓
  → 3 queues, 847 messages restored

Offering mongodb-ddb wishfully...
  → stone-jade-lake claimed (managed by zg-aws-bridge)
  → restoring data... ✓
  → 5 tables, 12,847 items restored

Offering mailpit wishfully...
  → stone-morning-mist claimed (managed by zg-aws-bridge)
  → restoring data... ✓
  → 1,203 emails restored

Offering loki wishfully...
  → stone-silver-stream claimed (managed by zg-aws-bridge)
  → restoring data... ✓
  → 14 days of logs restored

Bridge reconnecting to backends... ✓

AWS BRIDGE RESTORED
───────────────────────────────────────────────────
  Services:
    ✓ S3        seed-glorious-dawn
    ✓ SQS       redis-sqs (847 messages)
    ✓ DynamoDB  mongodb-ddb (12,847 items)
    ✓ SES       mailpit (1,203 emails)
    ✓ Logs      loki
    ✓ Secrets   keystone
    
  Your local cloud is back online.
```

### Disaster Recovery Time

| Component | Recovery Time |
|-----------|---------------|
| Bridge itself | < 1 minute |
| Redis (SQS/SNS) | 1-2 minutes |
| MongoDB (DynamoDB) | 2-5 minutes (depends on data size) |
| Loki (Logs) | 2-5 minutes |
| Mailpit (SES) | < 1 minute |
| Lambda functions | < 1 minute (code in S3) |

**Total garden recovery:** 5-10 minutes for a typical setup.

---

## Configuration

### Bridge Configuration File

```yaml
# /var/zg-aws-bridge/config.yaml

# General settings
general:
  log_level: info
  metrics_enabled: true

# Auto-provisioning
auto_provision:
  enabled: true
  
  # Default backends when auto-provisioning
  defaults:
    queue: redis
    document: mongodb
    logs: loki
    email: mailpit

# Service-specific configuration
services:
  s3:
    port: 4100
    # S3 always uses seed banks, no backend config needed
  
  sqs:
    port: 4101
    backend: auto                    # auto, redis, rabbitmq, builtin
    config:
      default_visibility_timeout_seconds: 30
      default_message_retention_days: 4
      max_message_size_kb: 256
      default_wait_time_seconds: 20
      dead_letter_queue_enabled: true
      dead_letter_max_receive_count: 3
  
  dynamodb:
    port: 4102
    backend: auto                    # auto, mongodb, sqlite
    config:
      default_read_capacity: 5
      default_write_capacity: 5
  
  secrets:
    port: 4103
    # Always uses Keystone, no backend config
  
  lambda:
    port: 4104
    config:
      default_memory_mb: 128
      default_timeout_seconds: 30
      warm_container_ttl_seconds: 300
      max_concurrent_executions: 10
  
  sns:
    port: 4105
    backend: auto                    # Shares with SQS
  
  ses:
    port: 4106
    backend: auto                    # auto, mailpit, smtp
    smtp:                            # Only if backend: smtp
      host: smtp.sendgrid.net
      port: 587
      username: apikey
      password: ${SENDGRID_API_KEY}
      tls: true
  
  logs:
    port: 4107
    backend: auto                    # auto, loki, file
    retention_days: 30
  
  ssm:
    port: 4108
    # Built-in, no backend config
  
  kms:
    port: 4109
    # Always uses Keystone, no backend config

# UI settings
ui:
  port: 4199
  enabled: true
```

### Bridge State File

```yaml
# /var/zg-aws-bridge/state.yaml
# This file is automatically managed and backed up

version: 1

enabled_services:
  s3:
    enabled: true
    backend:
      type: seed-bank
      name: seed-glorious-dawn
  
  sqs:
    enabled: true
    backend:
      type: redis
      offering_id: redis-abc123
      offering_name: redis-sqs
      managed: true
    config:
      default_visibility_timeout_seconds: 30
  
  dynamodb:
    enabled: true
    backend:
      type: mongodb
      offering_id: mongodb-def456
      offering_name: mongodb-ddb
      managed: true
  
  secrets:
    enabled: true
    backend:
      type: keystone
  
  lambda:
    enabled: false
  
  sns:
    enabled: false
  
  ses:
    enabled: true
    backend:
      type: mailpit
      offering_id: mailpit-ghi789
      offering_name: mailpit
      managed: true
  
  logs:
    enabled: true
    backend:
      type: loki
      offering_id: loki-jkl012
      offering_name: loki
      managed: true
  
  ssm:
    enabled: true
    backend:
      type: builtin
  
  kms:
    enabled: false

managed_offerings:
  - offering_id: redis-abc123
    service: sqs
    provisioned_at: 2026-01-20T10:00:00Z
  
  - offering_id: mongodb-def456
    service: dynamodb
    provisioned_at: 2026-01-20T10:05:00Z
  
  - offering_id: mailpit-ghi789
    service: ses
    provisioned_at: 2026-01-21T14:30:00Z
  
  - offering_id: loki-jkl012
    service: logs
    provisioned_at: 2026-01-22T09:15:00Z

last_updated: 2026-01-23T15:00:00Z
```

---

## Security

### Authentication

#### Dry Gardens

No authentication required. Credential check is a formality:

```
Access Key: zen-garden (or anything)
Secret Key: zen-garden (or anything)
```

#### Pond Gardens

Credentials derived from garden identity:

```
Access Key: {stone-id}
Secret Key: {derived-from-keystone}
```

The bridge validates requests against Keystone.

### Authorization

#### App Namespacing

All services enforce app namespaces:

| Service | Namespace |
|---------|-----------|
| S3 | `apps/{app-name}/` |
| SQS | Queue names prefixed with `{app-name}/` |
| DynamoDB | Table names prefixed with `{app-name}_` |
| Secrets | Secret names prefixed with `{app-name}/` |
| SSM | Parameter names prefixed with `/{app-name}/` |
| Lambda | Function names prefixed with `{app-name}-` |
| Logs | Log groups prefixed with `/{app-name}/` |

Apps cannot access other apps' resources.

### Pond Features

Services requiring Pond:

| Service | Reason |
|---------|--------|
| Secrets Manager | Encrypted storage |
| KMS | Key management |
| Presigned URLs | Cryptographic signatures |

Without Pond, these services return `501 Not Implemented`.

### Network Security

In Pond gardens:

- All inter-stone communication encrypted (mTLS)
- Backends only accept connections from bridge
- External access requires explicit configuration

---

## Offering Manifest

```yaml
# zg-aws-bridge.offering.yaml

name: zg-aws-bridge
version: 1.0.0
description: AWS-compatible API bridge for Zen Garden
repository: ghcr.io/zen-garden/aws-bridge

image: ghcr.io/zen-garden/aws-bridge:1.0.0

# Ports used by the bridge
ports:
  s3: 4100
  sqs: 4101
  dynamodb: 4102
  secrets: 4103
  lambda: 4104
  sns: 4105
  ses: 4106
  logs: 4107
  ssm: 4108
  kms: 4109
  ui: 4199

# Volumes for persistent state
volumes:
  - name: bridge-state
    path: /var/zg-aws-bridge
    size: 1Gi

# Soft dependencies - will use if available
wishes:
  - type: redis
    for: [sqs, sns]
    optional: true
  
  - type: mongodb
    for: [dynamodb]
    optional: true
  
  - type: loki
    for: [logs]
    optional: true
  
  - type: mailpit
    for: [ses]
    optional: true

# What this offering provides
provides:
  protocols:
    - zen-garden:s3
    - zen-garden:sqs
    - zen-garden:dynamodb
    - zen-garden:secrets
    - zen-garden:lambda
    - zen-garden:sns
    - zen-garden:ses
    - zen-garden:logs
    - zen-garden:ssm
    - zen-garden:kms
  
  capabilities:
    - aws-bridge
    - aws-s3
    - aws-sqs
    - aws-dynamodb
    - aws-lambda

# Capabilities this offering can use
uses_capabilities:
  - seed-bank           # For S3
  - container-runtime   # For Lambda
  - keystone           # For Secrets/KMS (Pond)

# Environment variables
environment:
  ZG_BRIDGE_CONFIG: /var/zg-aws-bridge/config.yaml
  ZG_BRIDGE_STATE: /var/zg-aws-bridge/state.yaml
  ZG_LOG_LEVEL: "${ZG_LOG_LEVEL:-info}"

# Health check
healthcheck:
  http: /api/v1/bridge/health
  port: 4199
  interval: 30s
  timeout: 10s
  retries: 3

# Resource requirements
resources:
  memory:
    minimum: 256Mi
    recommended: 512Mi
  cpu:
    minimum: 0.25
    recommended: 0.5

# Placement preferences
placement:
  # Prefer stones with good network connectivity
  prefer_capabilities: [fast-network]
  
  # Avoid stones that are resource-constrained
  avoid_labels: [low-memory]

# Migration support
migration:
  strategy: stateful-snapshot
  snapshot:
    paths:
      - /var/zg-aws-bridge/state.yaml
  restore:
    post_restore_healthcheck: true
```

---

## Client SDK

### The One-Line Migration

The Zen Garden AWS Bridge SDK enables existing AWS applications to run on a garden with **one line of code**:

```csharp
// Existing code - COMPLETELY UNTOUCHED
builder.Services.AddAWSService<IAmazonS3>();
builder.Services.AddAWSService<IAmazonSQS>();
builder.Services.AddAWSService<IAmazonDynamoDB>();
builder.Services.AddAWSService<IAmazonSecretsManager>();

// ADD THIS ONE LINE - that's the entire migration
builder.Services.AddZenGardenAwsBridge();
```

**What happens:**

1. Scans all registered `IAmazon*` services
2. Wraps each with a Zen Garden interceptor
3. Auto-detects garden vs AWS at runtime
4. Routes calls transparently

**Business logic changes: Zero.**

---

### How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│              AddZenGardenAwsBridge() EXECUTION                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   1. Scan ServiceCollection                                     │
│      Found: IAmazonS3 (registered by AddAWSService)             │
│      Found: IAmazonSQS (registered by AddAWSService)            │
│      Found: IAmazonDynamoDB (registered by AddAWSService)       │
│      Found: IAmazonSecretsManager (registered by AddAWSService) │
│                                                                 │
│   2. Remove original registrations                              │
│                                                                 │
│   3. Add wrapped registrations                                  │
│      IAmazonS3 → ZenGardenS3Wrapper(original)                   │
│      IAmazonSQS → ZenGardenSqsWrapper(original)                 │
│      IAmazonDynamoDB → ZenGardenDynamoWrapper(original)         │
│      IAmazonSecretsManager → ZenGardenSecretsWrapper(original)  │
│                                                                 │
│   4. On first resolve, auto-detect environment:                 │
│      → Garden detected? Route to bridge                         │
│      → Mapping file found? Route to mapped AWS resources        │
│      → Neither? Passthrough to original AWS                     │
│                                                                 │
│   5. App runs, completely unaware of the interception           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

### Resolution Modes

| Mode | Detection | Behavior |
|------|-----------|----------|
| **Garden** | mDNS discovery finds Zen Garden | Route to local AWS Bridge |
| **Mapped** | `zen-garden.yaml` file found | Route to real AWS with remapped resources |
| **Passthrough** | Neither found | Use original AWS SDK behavior |

The SDK checks in order: Garden → Mapped → Passthrough.

```
┌─────────────────────────────────────────────────────────────────┐
│                    RESOLUTION FLOW                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   App starts                                                    │
│       │                                                         │
│       ▼                                                         │
│   mDNS: Is there a garden?                                      │
│       │                                                         │
│       ├─── YES ──→ GARDEN MODE                                  │
│       │            Route all calls to AWS Bridge                │
│       │            S3 → http://stone.local:4100                 │
│       │            SQS → http://stone.local:4101                │
│       │                                                         │
│       ▼                                                         │
│   File: Is there zen-garden.yaml?                               │
│       │                                                         │
│       ├─── YES ──→ MAPPED MODE                                  │
│       │            Route to real AWS with remapped names        │
│       │            bucket "orders" → "prod-acme-orders"         │
│       │            table "users" → "prod-acme-users"            │
│       │                                                         │
│       ▼                                                         │
│   PASSTHROUGH MODE                                              │
│   Use original AWS SDK unchanged                                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

### C# SDK Implementation

#### Package

```bash
dotnet add package Xylin.ZenGarden.AwsBridge
```

#### Core Extension Method

```csharp
namespace Xylin.ZenGarden.AwsBridge
{
    public static class ZenGardenServiceCollectionExtensions
    {
        /// <summary>
        /// Scans all registered IAmazon* services and wraps them with 
        /// Zen Garden resolution. Add this AFTER all AddAWSService<T>() calls.
        /// </summary>
        public static IServiceCollection AddZenGardenAwsBridge(
            this IServiceCollection services,
            Action<ZenGardenOptions>? configure = null)
        {
            var options = new ZenGardenOptions();
            configure?.Invoke(options);
            
            // Auto-detect app name from assembly
            options.AppName ??= Assembly.GetEntryAssembly()?.GetName().Name?
                .ToLowerInvariant()
                .Replace(".", "-") ?? "app";
            
            // Register resolver (singleton, lazy initialization)
            services.AddSingleton<IZenGardenResolver>(_ => 
                ZenGardenResolver.AutoDetect(options));
            
            // Find all IAmazon* registrations and wrap them
            var amazonServices = services
                .Where(s => s.ServiceType.Name.StartsWith("IAmazon"))
                .ToList();
            
            foreach (var descriptor in amazonServices)
            {
                services.Remove(descriptor);
                services.Add(new ServiceDescriptor(
                    descriptor.ServiceType,
                    sp => WrapAmazonService(sp, descriptor),
                    descriptor.Lifetime
                ));
            }
            
            return services;
        }
        
        private static object WrapAmazonService(
            IServiceProvider sp, 
            ServiceDescriptor original)
        {
            var resolver = sp.GetRequiredService<IZenGardenResolver>();
            var inner = CreateOriginalInstance(sp, original);
            
            return original.ServiceType switch
            {
                var t when t == typeof(IAmazonS3) 
                    => resolver.Wrap((IAmazonS3)inner),
                var t when t == typeof(IAmazonSQS) 
                    => resolver.Wrap((IAmazonSQS)inner),
                var t when t == typeof(IAmazonDynamoDB) 
                    => resolver.Wrap((IAmazonDynamoDB)inner),
                var t when t == typeof(IAmazonSecretsManager) 
                    => resolver.Wrap((IAmazonSecretsManager)inner),
                var t when t == typeof(IAmazonSimpleNotificationService) 
                    => resolver.Wrap((IAmazonSimpleNotificationService)inner),
                var t when t == typeof(IAmazonSimpleEmailServiceV2) 
                    => resolver.Wrap((IAmazonSimpleEmailServiceV2)inner),
                var t when t == typeof(IAmazonLambda) 
                    => resolver.Wrap((IAmazonLambda)inner),
                var t when t == typeof(IAmazonCloudWatchLogs) 
                    => resolver.Wrap((IAmazonCloudWatchLogs)inner),
                var t when t == typeof(IAmazonSimpleSystemsManagement) 
                    => resolver.Wrap((IAmazonSimpleSystemsManagement)inner),
                var t when t == typeof(IAmazonKeyManagementService) 
                    => resolver.Wrap((IAmazonKeyManagementService)inner),
                _ => PassthroughWithWarning(original.ServiceType, inner)
            };
        }
    }
}
```

#### Options

```csharp
public class ZenGardenOptions
{
    /// <summary>
    /// Application name for namespacing. Auto-detected from assembly if not set.
    /// </summary>
    public string? AppName { get; set; }
    
    /// <summary>
    /// Force a specific resolution mode instead of auto-detection.
    /// </summary>
    public ZenGardenMode? Mode { get; set; }
    
    /// <summary>
    /// Path to mapping file for Mapped mode. Default: zen-garden.yaml
    /// </summary>
    public string MappingsFile { get; set; } = "zen-garden.yaml";
    
    /// <summary>
    /// Log all resource remapping (bucket names, queue URLs, etc.)
    /// </summary>
    public bool LogResolution { get; set; } = false;
    
    /// <summary>
    /// Timeout for garden detection via mDNS.
    /// </summary>
    public TimeSpan GardenDetectionTimeout { get; set; } = TimeSpan.FromSeconds(2);
}

public enum ZenGardenMode
{
    /// <summary>Auto-detect garden vs AWS</summary>
    Auto,
    /// <summary>Force garden mode (fail if no garden found)</summary>
    Garden,
    /// <summary>Force mapped mode (use mapping file)</summary>
    Mapped,
    /// <summary>Force passthrough (ignore garden even if present)</summary>
    Passthrough
}
```

#### Usage Examples

```csharp
// Minimal - auto-detect everything
builder.Services.AddZenGardenAwsBridge();

// With explicit app name
builder.Services.AddZenGardenAwsBridge(options =>
{
    options.AppName = "order-service";
});

// Force garden mode (fail if no garden)
builder.Services.AddZenGardenAwsBridge(options =>
{
    options.Mode = ZenGardenMode.Garden;
});

// Force passthrough (use real AWS)
builder.Services.AddZenGardenAwsBridge(options =>
{
    options.Mode = ZenGardenMode.Passthrough;
});

// With logging for debugging
builder.Services.AddZenGardenAwsBridge(options =>
{
    options.LogResolution = true;
});
```

---

### Transparent Wrapping

The wrappers intercept AWS SDK calls and remap resources transparently:

#### S3 Wrapper

```csharp
internal class ZenGardenS3Wrapper : IAmazonS3
{
    private readonly IAmazonS3 _inner;
    private readonly string _appName;
    private readonly string _targetBucket;
    
    public async Task<PutObjectResponse> PutObjectAsync(
        PutObjectRequest request, 
        CancellationToken ct = default)
    {
        // Remap bucket and key transparently
        var rewritten = Clone(request);
        rewritten.BucketName = _targetBucket;  // "garden" in garden mode
        rewritten.Key = $"apps/{_appName}/{request.BucketName}/{request.Key}";
        
        return await _inner.PutObjectAsync(rewritten, ct);
    }
    
    public async Task<GetObjectResponse> GetObjectAsync(
        GetObjectRequest request,
        CancellationToken ct = default)
    {
        var rewritten = Clone(request);
        rewritten.BucketName = _targetBucket;
        rewritten.Key = $"apps/{_appName}/{request.BucketName}/{request.Key}";
        
        return await _inner.GetObjectAsync(rewritten, ct);
    }
    
    public async Task<ListObjectsV2Response> ListObjectsV2Async(
        ListObjectsV2Request request,
        CancellationToken ct = default)
    {
        var prefix = $"apps/{_appName}/{request.BucketName}/{request.Prefix ?? ""}";
        var rewritten = Clone(request);
        rewritten.BucketName = _targetBucket;
        rewritten.Prefix = prefix;
        
        var response = await _inner.ListObjectsV2Async(rewritten, ct);
        
        // Strip prefix from returned keys
        foreach (var obj in response.S3Objects)
        {
            obj.Key = StripAppPrefix(obj.Key, request.BucketName);
        }
        
        return response;
    }
    
    // ... implements full IAmazonS3 interface
}
```

#### SQS Wrapper

```csharp
internal class ZenGardenSqsWrapper : IAmazonSQS
{
    private readonly IAmazonSQS _inner;
    private readonly string _appName;
    private readonly IQueueResolver _queueResolver;
    
    public async Task<SendMessageResponse> SendMessageAsync(
        SendMessageRequest request,
        CancellationToken ct = default)
    {
        // Resolve queue URL (handles both short names and full URLs)
        request.QueueUrl = await _queueResolver.ResolveAsync(request.QueueUrl);
        
        return await _inner.SendMessageAsync(request, ct);
    }
    
    public async Task<ReceiveMessageResponse> ReceiveMessageAsync(
        ReceiveMessageRequest request,
        CancellationToken ct = default)
    {
        request.QueueUrl = await _queueResolver.ResolveAsync(request.QueueUrl);
        
        return await _inner.ReceiveMessageAsync(request, ct);
    }
    
    // ... implements full IAmazonSQS interface
}

internal class QueueResolver : IQueueResolver
{
    public async Task<string> ResolveAsync(string queueUrl)
    {
        // "jobs" → "http://bridge:4101/sqs/my-app/jobs" (garden)
        // "https://sqs.../my-queue" → lookup in mappings (mapped mode)
        // Full URL → passthrough (passthrough mode)
    }
}
```

#### DynamoDB Wrapper

```csharp
internal class ZenGardenDynamoWrapper : IAmazonDynamoDB
{
    private readonly IAmazonDynamoDB _inner;
    private readonly string _appName;
    
    public async Task<PutItemResponse> PutItemAsync(
        PutItemRequest request,
        CancellationToken ct = default)
    {
        // Remap table name
        request.TableName = $"{_appName}_{request.TableName}";
        
        return await _inner.PutItemAsync(request, ct);
    }
    
    public async Task<GetItemResponse> GetItemAsync(
        GetItemRequest request,
        CancellationToken ct = default)
    {
        request.TableName = $"{_appName}_{request.TableName}";
        
        return await _inner.GetItemAsync(request, ct);
    }
    
    // ... implements full IAmazonDynamoDB interface
}
```

---

### Mapping File

For deploying back to real AWS (or different AWS accounts), create a mapping file:

```yaml
# zen-garden.yaml
version: 1

app: order-service

# Environment info
environment: production
aws_region: us-east-1

mappings:
  # S3: app bucket names → real AWS buckets
  s3:
    orders:
      bucket: prod-acme-orders
      prefix: ""
    archives:
      bucket: prod-acme-archives
      prefix: "order-service/"

  # SQS: app queue names → real AWS queue URLs
  sqs:
    jobs:
      url: https://sqs.us-east-1.amazonaws.com/123456789012/prod-order-jobs
    notifications:
      url: https://sqs.us-east-1.amazonaws.com/123456789012/prod-order-notifications
    dead-letter:
      url: https://sqs.us-east-1.amazonaws.com/123456789012/prod-order-dlq

  # DynamoDB: app table names → real AWS table names
  dynamodb:
    orders: prod-acme-orders
    users: prod-acme-users
    sessions: prod-acme-sessions-v2

  # Secrets Manager: app secret names → real AWS secret ARNs/names
  secrets:
    database/password: prod/order-service/db-password
    api-keys/stripe: prod/order-service/stripe-key

  # SNS: app topic names → real AWS topic ARNs
  sns:
    order-events: arn:aws:sns:us-east-1:123456789012:prod-order-events

  # Parameter Store: app parameter paths → real AWS parameter paths
  ssm:
    prefix: /prod/order-service/
```

---

### Startup Logging

```
info: Xylin.ZenGarden.AwsBridge[0]
      Zen Garden AWS Bridge initializing...
info: Xylin.ZenGarden.AwsBridge[0]
      Found 4 AWS services to wrap:
        - IAmazonS3
        - IAmazonSQS
        - IAmazonDynamoDB
        - IAmazonSecretsManager
info: Xylin.ZenGarden.AwsBridge[0]
      Detecting environment...
info: Xylin.ZenGarden.AwsBridge[0]
      Garden detected at stone-jade-lake.local
info: Xylin.ZenGarden.AwsBridge[0]
      AWS Bridge endpoint: http://stone-jade-lake.local:4100
info: Xylin.ZenGarden.AwsBridge[0]
      App namespace: order-service
info: Xylin.ZenGarden.AwsBridge[0]
      Ready. All AWS calls will route through Zen Garden.
```

---

### Alternative: Direct Offering Resolution

For new code or when you want explicit control:

```csharp
using Xylin.ZenGarden.AwsBridge;

// Type-safe offering resolution
var s3 = await AwsBridge.Offering<IAmazonS3>("zen-garden:s3//my-app");
var sqs = await AwsBridge.Offering<IAmazonSQS>("zen-garden:sqs//my-app");
var dynamo = await AwsBridge.Offering<IAmazonDynamoDB>("zen-garden:dynamodb//my-app");

// Use standard AWS SDK interfaces
await s3.PutObjectAsync(new PutObjectRequest
{
    BucketName = "garden",
    Key = "data/file.txt",
    ContentBody = "Hello, World!"
});
```

With dependency injection:

```csharp
// Register specific offerings
builder.Services.AddZenGardenOffering<IAmazonS3>("zen-garden:s3//order-service");
builder.Services.AddZenGardenOffering<IAmazonSQS>("zen-garden:sqs//order-service");

// Inject and use
public class OrderService
{
    private readonly IAmazonS3 _s3;
    private readonly IAmazonSQS _sqs;
    
    public OrderService(IAmazonS3 s3, IAmazonSQS sqs)
    {
        _s3 = s3;
        _sqs = sqs;
    }
}
```

---

### Cross-Language SDKs

The same pattern applies across languages:

#### Python

```python
# pip install zen-garden-aws-bridge

# Patch boto3 clients automatically
import zen_garden.aws_bridge
zen_garden.aws_bridge.patch()

# Existing boto3 code works unchanged
import boto3
s3 = boto3.client('s3')
s3.put_object(Bucket='orders', Key='file.txt', Body=b'hello')

# Auto-routes to garden if detected, AWS otherwise
```

#### Node.js / TypeScript

```typescript
// npm install @zen-garden/aws-bridge

// Patch AWS SDK
import { patch } from '@zen-garden/aws-bridge';
patch();

// Existing code works unchanged
import { S3Client, PutObjectCommand } from '@aws-sdk/client-s3';
const s3 = new S3Client({});
await s3.send(new PutObjectCommand({
    Bucket: 'orders',
    Key: 'file.txt',
    Body: 'hello'
}));
```

#### Go

```go
// go get github.com/xylin/zen-garden-aws-bridge

import (
    "github.com/aws/aws-sdk-go-v2/service/s3"
    zg "github.com/xylin/zen-garden-aws-bridge"
)

// Wrap existing client
s3Client := s3.NewFromConfig(cfg)
wrappedS3 := zg.WrapS3(s3Client, "order-service")

// Or auto-create with detection
s3Client := zg.NewS3Client("order-service")
```

#### Rust

```rust
// Cargo.toml: zen-garden-aws-bridge = "0.1"

use zen_garden_aws_bridge::prelude::*;

// Auto-detecting client
let s3 = ZenGarden::s3("order-service").await?;

// Use standard AWS SDK types
s3.put_object()
    .bucket("orders")
    .key("file.txt")
    .body(ByteStream::from_static(b"hello"))
    .send()
    .await?;
```

---

### Migration Guide

#### Step 1: Add Package

```bash
dotnet add package Xylin.ZenGarden.AwsBridge
```

#### Step 2: Add One Line

```csharp
// Program.cs - add at the end of service configuration
builder.Services.AddZenGardenAwsBridge();
```

#### Step 3: Set Up Garden

```bash
# On your homelab machines
garden-rake offer zg-aws-bridge

# Enable the services you need (or let them auto-provision)
garden-rake bridge enable sqs
garden-rake bridge enable dynamodb
```

#### Step 4: Run

```bash
# Your app auto-detects the garden
dotnet run
```

#### Summary

| Step | Command/Change | Time |
|------|----------------|------|
| Add package | `dotnet add package Xylin.ZenGarden.AwsBridge` | 10 sec |
| Add one line | `builder.Services.AddZenGardenAwsBridge();` | 30 sec |
| Set up garden | `garden-rake offer zg-aws-bridge` | 2 min |
| Run | `dotnet run` | — |
| **Total** | | **~3 minutes** |

**Code changes to business logic: Zero**
**AWS bill: $400/month → $0/month**

---

### Environment Variables

For zero-code configuration:

| Variable | Description |
|----------|-------------|
| `ZEN_GARDEN_ENABLED` | Set to `true` to enable interception |
| `ZEN_GARDEN_APP` | Application name for namespacing |
| `ZEN_GARDEN_MODE` | `auto`, `garden`, `mapped`, or `passthrough` |
| `ZEN_GARDEN_MAPPINGS` | Path to mapping file |
| `ZEN_GARDEN_LOG_RESOLUTION` | Set to `true` for verbose logging |

```bash
# Force garden mode via environment
export ZEN_GARDEN_MODE=garden
dotnet run

# Force passthrough (real AWS) via environment
export ZEN_GARDEN_MODE=passthrough
dotnet run
```

---

## References

- [Storage API Specification](zen-garden-spec-storage-api.md) — S3 implementation details
- [Seed Bank Specification](zen-garden-spec-seed-banks.md) — Storage backend for S3
- [Cultivation Specification](zen-garden-spec-cultivation.md) — Backup/restore mechanics
- [Ceremony Specification](zen-garden-spec-ceremonies.md) — Distributed operations
- [Security Specification](zen-garden-spec-security.md) — Pond, Keystone

---

## Future Enhancements

### Planned Services

| Service | Description | Priority |
|---------|-------------|----------|
| API Gateway | HTTP API management | Medium |
| Step Functions | Workflow orchestration | Medium |
| EventBridge | Event routing | Medium |
| Cognito | User authentication | Low |
| CloudWatch Metrics | Metrics and alarms | Low |
| Kinesis | Data streaming | Low |

### Planned Features

| Feature | Description | Priority |
|---------|-------------|----------|
| Multi-bridge | Multiple bridges for HA | Medium |
| Cross-garden | Access resources in other gardens | Low |
| Cloud sync | Sync with real AWS | Low |
| Terraform provider | IaC support | Medium |

---

**Last Updated:** January 2026  
**Status:** Proposal — pending review and implementation
