---
audience: [developer, ai]
doc_type: reference
last_verified: 2026-03-22
---

# S3 API Reference — Zen Garden Storage Gateway

**Sources**: STORAGE-0016 (Unified S3 Storage Gateway), STORAGE-0009 (Managed Storage)
**Purpose**: Reference for the S3-compatible protocol exposed per storage bank by Moss

---

## Endpoint Model (STORAGE-0016)

Each storage bank has a **dedicated S3 listener** on its own port. Moss arms one
listener per locally-mounted replica set when it starts or when storage is added.

```
http://{stone-hostname}:{s3-port}/{bucket}/{key}
```

Standard S3 clients (AWS SDK, MinIO SDK, rclone, Cyberduck) connect to
`host:port` and operate at root `/` with no path prefix.

### Port Assignment

Ports are assigned from a configurable range (default: `23400–23499`), deterministic
by replica set display name. Assignments persist across restarts. If the range is
exhausted, Moss skips the S3 listener for that storage and falls back to the
compatibility path on port 7185.

**Discover ports** via:
- `GET /api/v1/stone/storage/s3/ports` — stone-local diagnostic endpoint
- Garden SSE tools stream (`GET /api/v1/stone/storage/stream`) — `connection.port`
  and `connection.uris` in each storage tool snapshot

### Legacy Compatibility Path (port 7185)

The original routes remain on the main HTTP port for backward compatibility and
internal use. They use the same unified namespace as the per-port listeners.

```
GET    /api/v1/storage/s3              → ListBuckets
PUT    /api/v1/storage/s3/:bucket      → CreateBucket
GET    /api/v1/storage/s3/:bucket      → ListObjects (V1/V2)
PUT    /api/v1/storage/s3/:bucket/*key → PutObject / CopyObject
GET    /api/v1/storage/s3/:bucket/*key → GetObject (Range supported)
HEAD   /api/v1/storage/s3/:bucket/*key → HeadObject
DELETE /api/v1/storage/s3/:bucket/*key → DeleteObject
```

Storage selection on the legacy path (not needed on per-port listeners):
- Header: `X-Seed-Bank: {name}`
- Query param: `?seed-bank={name}`

---

## Unified Namespace

S3 objects share the **mount root** with native REST and WebDAV files. An S3 bucket
is a directory at the storage mount root. Writing via S3 and writing via
`PUT /api/v1/garden/storage/{name}/fs/{path}` reach the same bytes.

```
{mount}/
├── .zen-garden/
│   ├── manifest.json        ← Infrastructure (excluded from changelog)
│   ├── changelog.jsonl
│   └── meta/                ← S3 metadata sidecars (excluded from changelog)
│       └── photos/
│           └── IMG001.jpg.json
├── photos/                  ← S3 bucket = directory at mount root
│   └── IMG001.jpg           ← Readable via S3, REST, WebDAV, and Windows Explorer
```

S3 writes generate changelog entries → automatic replication to dormant replicas.

---

## Supported Operations

### ListBuckets

```
GET / HTTP/1.1
```

Returns all directories at the mount root as S3 buckets.

**Response**: 200 OK — `ListAllMyBucketsResult` XML

---

### CreateBucket

```
PUT /{bucket} HTTP/1.1
```

Creates a directory at the mount root. Auto-create on first `PutObject` is also
retained.

**Response**: 200 OK on success

---

### ListObjects V1

```
GET /{bucket}?prefix=&delimiter=&marker=&max-keys= HTTP/1.1
```

**Query Parameters**:
| Parameter | Type | Description |
|-----------|------|-------------|
| prefix | String | Only returns objects with specified prefix |
| delimiter | String | Delimiter between prefix and rest of object name |
| marker | String | Beginning index for list of objects returned |
| max-keys | Integer | Maximum number of keys to return. Default/max: 1000 |

**Response**: 200 OK

**Response XML**:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Name>bucket-name</Name>
    <Prefix>prefix</Prefix>
    <Marker>marker</Marker>
    <MaxKeys>1000</MaxKeys>
    <Delimiter>/</Delimiter>
    <IsTruncated>false</IsTruncated>
    <Contents>
        <Key>object-key</Key>
        <LastModified>2024-01-01T00:00:00.000Z</LastModified>
        <ETag>"d41d8cd98f00b204e9800998ecf8427e"</ETag>
        <Size>1234</Size>
        <StorageClass>STANDARD</StorageClass>
    </Contents>
    <CommonPrefixes>
        <Prefix>prefix/</Prefix>
    </CommonPrefixes>
</ListBucketResult>
```

---

### ListObjects V2

```
GET /{bucket}?list-type=2&prefix=&delimiter=&start-after=&continuation-token=&max-keys= HTTP/1.1
```

Activated by `list-type=2`. Default when `list-type` is absent: V1.

**Additional query parameters (V2 only)**:
| Parameter | Type | Description |
|-----------|------|-------------|
| list-type | Integer | Must be `2` to activate V2 |
| start-after | String | Start listing after this key |
| continuation-token | String | Opaque token from previous response |

**Additional response fields (V2 only)**:
| Field | Description |
|-------|-------------|
| KeyCount | Number of keys returned in this response |
| ContinuationToken | Token used for this request |
| NextContinuationToken | Token for next page (only if IsTruncated=true) |
| StartAfter | Echoed from request |

---

### PutObject

```
PUT /{bucket}/{key} HTTP/1.1
```

**Request Headers**:
| Header | Description | Required |
|--------|-------------|----------|
| Content-MD5 | Base64-encoded MD5 hash | No |
| Content-Type | MIME type. Default: `application/octet-stream` | No |
| x-amz-meta-{name} | Custom metadata. Persisted in sidecar; returned on GET/HEAD | No |

**Response**:
- 200 OK on success
- `ETag` response header with MD5 hash of content

---

### CopyObject

```
PUT /{dest-bucket}/{dest-key} HTTP/1.1
x-amz-copy-source: /{source-bucket}/{source-key}
```

Copies an object within or across storages. When source and destination are on the
same storage, the copy is a direct filesystem operation. When source is on a remote
stone, Moss fetches it via proxy and writes locally.

**Response**: 200 OK

**Response XML**:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<CopyObjectResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <LastModified>2026-03-22T10:00:00.000Z</LastModified>
    <ETag>"d41d8cd98f00b204e9800998ecf8427e"</ETag>
</CopyObjectResult>
```

---

### GetObject

```
GET /{bucket}/{key} HTTP/1.1
```

**Request Headers**:
| Header | Description | Required |
|--------|-------------|----------|
| Range | Byte range: `bytes=N-M` | No |
| If-Match | Return only if ETag matches | No |
| If-None-Match | Return only if ETag does not match | No |
| If-Modified-Since | Return only if modified after timestamp | No |
| If-Unmodified-Since | Return only if not modified after timestamp | No |

When a `Range` header is present, the response is HTTP 206 Partial Content.

**Response Headers**:
| Header | Description |
|--------|-------------|
| Accept-Ranges | Always `bytes` |
| Content-Range | `bytes N-M/total` (only if range requested) |
| ETag | Entity tag (MD5 hash) |
| Content-Length | Size of returned body (range size if partial) |
| Content-Type | MIME type |
| Last-Modified | Last modification timestamp |
| x-amz-meta-{name} | Custom metadata (if set on PUT) |

---

### HeadObject

```
HEAD /{bucket}/{key} HTTP/1.1
```

Returns the same headers as `GetObject` with no body. Accepts the same conditional
request headers (`If-Match`, `If-None-Match`, `If-Modified-Since`,
`If-Unmodified-Since`).

---

### DeleteObject

```
DELETE /{bucket}/{key} HTTP/1.1
```

**Response**: 204 No Content on success

Accepts conditional headers (`If-Match`, `If-None-Match`).

---

## Multipart Upload

For large objects. The MinIO .NET SDK uses multipart automatically for objects
larger than 16 MB.

Parts are staged under `.zen-garden/multipart/{upload_id}/` and excluded from the
changelog. Only the final assembled object enters the changelog and triggers
replication.

Incomplete uploads are garbage-collected after 24 hours.

### Initiate

```
POST /{bucket}/{key}?uploads HTTP/1.1
```

**Response**: 200 OK

```xml
<?xml version="1.0" encoding="UTF-8"?>
<InitiateMultipartUploadResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Bucket>photos</Bucket>
    <Key>IMG001.jpg</Key>
    <UploadId>019a5aff-79cb-7815-8dae-3700a698f840</UploadId>
</InitiateMultipartUploadResult>
```

### Upload Part

```
PUT /{bucket}/{key}?partNumber={n}&uploadId={id} HTTP/1.1
```

`partNumber` is 1-based. Parts must be >= 5 MB except for the last part.

**Response**: 200 OK — `ETag` response header for this part

### Complete

```
POST /{bucket}/{key}?uploadId={id} HTTP/1.1
Content-Type: application/xml

<CompleteMultipartUpload>
    <Part>
        <PartNumber>1</PartNumber>
        <ETag>"etag-of-part-1"</ETag>
    </Part>
    <Part>
        <PartNumber>2</PartNumber>
        <ETag>"etag-of-part-2"</ETag>
    </Part>
</CompleteMultipartUpload>
```

Assembles parts in order, writes the final object, then cleans up staging.

**Response**: 200 OK

```xml
<?xml version="1.0" encoding="UTF-8"?>
<CompleteMultipartUploadResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Location>http://stone-01.local:23400/photos/IMG001.jpg</Location>
    <Bucket>photos</Bucket>
    <Key>IMG001.jpg</Key>
    <ETag>"d41d8cd98f00b204e9800998ecf8427e"</ETag>
</CompleteMultipartUploadResult>
```

### Abort

```
DELETE /{bucket}/{key}?uploadId={id} HTTP/1.1
```

Cleans up staged parts. **Response**: 204 No Content

---

## Presigned URLs (Moss-Native)

Moss generates time-limited, operation-scoped access tokens using a native HMAC
scheme. These are not AWS SigV4 presigned URLs.

### Generate

```
POST /api/v1/storage/s3/presign HTTP/1.1
Content-Type: application/json

{
  "bucket": "snap-vault-photos",
  "key": "IMG001.jpg",
  "method": "GET",
  "expires_in_secs": 3600
}
```

`method` defaults to `"GET"`. `expires_in_secs` defaults to `3600`.

**Response**: 200 OK

```json
{
  "url": "http://stone-01.local:23400/snap-vault-photos/IMG001.jpg?X-Moss-Token={token}&X-Moss-Expires={timestamp}",
  "expires_at": "2026-03-22T14:00:00Z"
}
```

The URL points to the per-storage S3 port when one is available, or falls back to
the legacy path on port 7185.

### Token Format

```
HMAC-SHA256(secret, "{METHOD}\n{bucket}/{key}\n{expires_unix_timestamp}")
```

Secret derivation (two-tier):
- **Pond active**: `SHA256(ca_fingerprint + ":moss-presign-v1")` — garden-scoped;
  presigned URLs survive storage migration between stones in the pond.
- **No pond**: `SHA256(stone_id + ":moss-presign-v1")` — stone-scoped fallback.

### Validation

When `X-Moss-Token` and `X-Moss-Expires` query parameters are present on any S3
request, Moss validates the token before processing. Invalid or expired tokens
return 403 Forbidden.

---

## Port Catalog Endpoint

```
GET /api/v1/stone/storage/s3/ports HTTP/1.1
```

Returns the S3 port assignment for all known replica sets on this stone.

**Response**:
```json
{
  "ports": {
    "storage": {
      "port": 23400,
      "replica_set_id": "019a5aff-79cb-7815-8dae-3700a698f840",
      "status": "listening"
    },
    "images": {
      "port": 23401,
      "replica_set_id": "019b6c00-1234-7000-abcd-000000000001",
      "status": "listening"
    },
    "archive": {
      "port": 23402,
      "replica_set_id": "019c7d11-5678-7000-efab-000000000002",
      "status": "unavailable"
    }
  },
  "range": {
    "base": 23400,
    "size": 100
  }
}
```

| Status | Meaning |
|--------|---------|
| `listening` | Port armed, storage mounted, accepting requests |
| `unavailable` | Port armed, storage removed, returning 503 |
| `disabled` | Port not armed (range exhausted or manually disabled) |

Consumers that subscribe to the Garden SSE tools stream do not need to poll this
endpoint — S3 port is present in each storage tool snapshot as `connection.port`
and `connection.uris`.

---

## Storage Removal: Graceful Degradation

When a storage device is physically removed, the S3 listener remains armed on its
port. All requests return 503 with S3-compatible XML:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>ServiceUnavailable</Code>
    <Message>Storage replica set is temporarily unavailable</Message>
    <Resource>{bucket}/{key}</Resource>
    <RequestId>{request_id}</RequestId>
</Error>
```

The port is released only after the storage is permanently removed and a configurable
hold period (default: 24 hours) has elapsed.

---

## HTTP Status Codes

| Code | Status | Description |
|------|--------|-------------|
| 200 | OK | Request succeeded |
| 201 | Created | Resource created |
| 204 | No Content | Request succeeded, no content returned |
| 206 | Partial Content | Range read response |
| 304 | Not Modified | Conditional request, resource not modified |
| 400 | Bad Request | Malformed request |
| 403 | Forbidden | Access denied or presigned token invalid/expired |
| 404 | Not Found | Object not found |
| 409 | Conflict | Conflict (e.g., bucket already exists) |
| 412 | Precondition Failed | Conditional request failed |
| 416 | Range Not Satisfiable | Requested byte range not valid |
| 500 | Internal Server Error | Server error |
| 503 | Service Unavailable | Storage device removed |

---

## Error Response Format

All error responses use S3-compatible XML regardless of which path triggered them.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>NoSuchKey</Code>
    <Message>Object 'photos/IMG001.jpg' not found</Message>
    <Resource>/photos/IMG001.jpg</Resource>
    <RequestId>019a5aff-79cb-7815-8dae-3700a698f840</RequestId>
</Error>
```

| Code | HTTP Status | Trigger |
|------|-------------|---------|
| `NoSuchBucket` | 404 | Bucket (directory) does not exist |
| `NoSuchKey` | 404 | Object does not exist |
| `InvalidBucketName` | 400 | Bucket name fails validation |
| `EntityTooLarge` | 413 | Upload exceeds size limit |
| `InvalidRange` | 416 | Range header not satisfiable |
| `PreconditionFailed` | 412 | Conditional header not satisfied |
| `NotModified` | 304 | `If-None-Match` / `If-Modified-Since` matched |
| `ServiceUnavailable` | 503 | Storage device removed |

---

## Implementation Notes

### Namespace Unification

The `ContentStore::write()` changelog guard excludes paths under `.zen-garden/`.
Since S3 objects now write to the mount root (`{bucket}/{key}`), they are
automatically included in the changelog and replicated via STORAGE-0006 machinery.
Metadata sidecars (`.zen-garden/meta/`) remain excluded — they are derived data.

### ETag Generation

MD5 hash computed during upload. Stored in the metadata sidecar at
`.zen-garden/meta/{bucket}/{key}.json`. Returned in GET, HEAD, and PUT responses.

### Custom Metadata

`x-amz-meta-*` headers on PutObject are persisted under `custom_metadata` in the
sidecar. Returned on GetObject and HeadObject as response headers.

### Bucket Naming Convention

Koan clients use `{AppIdentity.Code}-{container}` naming (e.g., `snap-vault-photos`)
to prevent collisions with user-created directories visible through native and WebDAV
APIs. Moss does not enforce naming — collision prevention is a consumer responsibility.
