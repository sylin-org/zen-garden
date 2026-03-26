---
audience: [operator, developer]
doc_type: guide
status: current
last_verified: 2026-03-25
---

# S3 Presigned URLs

**Share time-limited access to S3 objects without exposing credentials.**

---

## Overview

Presigned URLs grant temporary, operation-scoped access to objects in Zen Garden storage banks. A presigned URL encodes the bucket, key, HTTP method, and an expiration timestamp, signed with an HMAC-SHA256 token. Anyone with the URL can perform the specified operation until the token expires — no authentication headers required.

Use presigned URLs to:
- Share a download link with a user or application that has no Moss credentials
- Allow a one-time upload to a specific key
- Embed time-limited media links in web pages or emails

---

## Generate a Presigned URL

Send a `POST` request to the presign endpoint on any stone:

```
POST /api/v1/storage/s3/presign
```

### Request Body

```json
{
  "bucket": "my-bucket",
  "key": "reports/q1-summary.pdf",
  "method": "GET",
  "expires_in_secs": 3600
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bucket` | string | *(required)* | Target bucket name |
| `key` | string | *(required)* | Object key (path) |
| `method` | string | `"GET"` | HTTP method the URL authorizes (`GET`, `PUT`, `DELETE`, `HEAD`) |
| `expires_in_secs` | integer | `3600` | Seconds until the URL expires (max depends on your use case) |

### Response

```json
{
  "url": "http://stone-crystal-forest.local:23400/my-bucket/reports/q1-summary.pdf?X-Moss-Token=a1b2c3...&X-Moss-Expires=1742918400",
  "expires_at": "2026-03-25T18:00:00+00:00"
}
```

The `url` field is ready to use. If the stone has a dedicated S3 listener port for the storage bank, the URL points to that port. Otherwise it falls back to the Moss API port (`7185`) with the `/api/v1/storage/s3/` path prefix.

---

## Use the URL

The generated URL works with any HTTP client. No auth headers are needed.

### Download an object

```bash
curl -o q1-summary.pdf \
  "http://stone-crystal-forest.local:23400/my-bucket/reports/q1-summary.pdf?X-Moss-Token=a1b2c3...&X-Moss-Expires=1742918400"
```

### Upload an object

Generate a presigned URL with `"method": "PUT"`, then upload:

```bash
curl -X PUT --data-binary @report.pdf \
  "http://stone-crystal-forest.local:23400/my-bucket/reports/q1-summary.pdf?X-Moss-Token=d4e5f6...&X-Moss-Expires=1742918400"
```

The token is scoped to the exact method, bucket, key, and expiration timestamp. A `GET` token cannot be used for `PUT`, and vice versa.

---

## HMAC Secret Derivation

Presigned URLs are signed with HMAC-SHA256. The signing secret is derived from one of two sources, depending on whether the stone belongs to a pond:

| Condition | Key material | Scope |
|-----------|-------------|-------|
| Stone is in an active pond | CA certificate fingerprint | **Garden-scoped** — any stone in the pond validates the token |
| Stone is standalone | Stone ID | **Stone-scoped** — only the issuing stone validates the token |

The derivation formula is `SHA-256(key_material + ":moss-presign-v1")`.

When a stone joins a pond, presigned URLs automatically become portable across all stones in that pond. URLs issued before joining a pond (using the stone ID) stop working after the secret changes.

---

## Token Format

The signed message has the structure:

```
{METHOD}\n{bucket}/{key}\n{expires_timestamp}
```

The resulting HMAC digest is hex-encoded and appended to the URL as the `X-Moss-Token` query parameter. The expiration Unix timestamp is appended as `X-Moss-Expires`.

On each request, Moss recomputes the expected token from the URL components and the current secret. Validation uses constant-time comparison to prevent timing attacks.

---

## Security Considerations

- **Expiration**: set `expires_in_secs` to the shortest practical window. A one-hour default is reasonable for interactive downloads; use shorter values (60-300 seconds) for automated workflows.
- **Method scope**: each token authorizes exactly one HTTP method. Generate separate URLs for read and write access.
- **Pond migration**: when a stone joins or leaves a pond, the signing secret changes. All previously issued URLs become invalid.
- **Transport**: presigned URLs use plain HTTP by default. For cross-network use, consider placing Moss behind a TLS reverse proxy or using pond mTLS (port 7183).
- **URL leakage**: treat presigned URLs like passwords for their validity window. Log access if auditability matters.

---

## Related

- [Storage guide](storage.md) — setting up storage banks and S3 access
- [API reference](../../.agentic/reference/api-endpoints.md) — full S3 gateway endpoint list
