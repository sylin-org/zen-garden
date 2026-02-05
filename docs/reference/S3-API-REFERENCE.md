# S3 API Reference for Zen Garden Storage Gateway

**Source**: Ceph RadosGW S3 API Documentation (CC-BY-SA-3.0)
**Purpose**: Local reference for implementing S3-compatible seed bank storage gateway

---

## Object Operations

### PUT Object

Adds an object to a bucket.

**Syntax**:
```
PUT /{bucket}/{object} HTTP/1.1
```

**Request Headers**:
| Header | Description | Valid Values | Required |
|--------|-------------|--------------|----------|
| content-md5 | Base64 encoded MD-5 hash | String | No |
| content-type | MIME type | Any MIME type. Default: binary/octet-stream | No |
| x-amz-meta-<...> | User metadata | String up to 8kb | No |

**Response**:
- 200 OK on success
- Returns ETag header with MD5 hash of content

---

### GET Object

Retrieves an object from a bucket.

**Syntax**:
```
GET /{bucket}/{object} HTTP/1.1
```

**Request Headers**:
| Header | Description | Valid Values | Required |
|--------|-------------|--------------|----------|
| range | Range of object to retrieve | Range: bytes=beginbyte-endbyte | No |
| if-modified-since | Get only if modified since timestamp | Timestamp | No |
| if-unmodified-since | Get only if not modified since timestamp | Timestamp | No |
| if-match | Get only if ETag matches | Entity Tag | No |
| if-none-match | Get only if ETag doesn't match | Entity Tag | No |

**Response Headers**:
| Header | Description |
|--------|-------------|
| Content-Range | Data range (only if range requested) |
| ETag | Entity tag (MD5 hash) |
| Content-Length | Size in bytes |
| Content-Type | MIME type |
| Last-Modified | Last modification timestamp |

---

### HEAD Object (Get Object Info)

Returns information about object without the data payload.

**Syntax**:
```
HEAD /{bucket}/{object} HTTP/1.1
```

**Request Headers**: Same as GET Object

**Response Headers**: Same as GET Object (headers only, no body)

---

### DELETE Object

Removes an object.

**Syntax**:
```
DELETE /{bucket}/{object} HTTP/1.1
```

**Response**:
- 204 No Content on success

---

## Bucket Operations

### GET Bucket (List Objects)

Returns a list of bucket objects.

**Syntax**:
```
GET /{bucket}?max-keys=25 HTTP/1.1
```

**Query Parameters**:
| Parameter | Type | Description |
|-----------|------|-------------|
| prefix | String | Only returns objects with specified prefix |
| delimiter | String | Delimiter between prefix and rest of object name |
| marker | String | Beginning index for list of objects returned |
| max-keys | Integer | Maximum number of keys to return. Default: 1000 |

**Response**: 200 OK

**Response XML Structure**:
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

**Response Entities**:
| Entity | Type | Description |
|--------|------|-------------|
| ListBucketResult | Container | Container for list of objects |
| Name | String | Bucket name |
| Prefix | String | Prefix for object keys |
| Marker | String | Beginning index for objects returned |
| MaxKeys | Integer | Maximum keys returned |
| Delimiter | String | Delimiter used |
| IsTruncated | Boolean | True if only subset returned |
| CommonPrefixes | Container | Objects with same prefix appear here |
| Contents | Container | Container for each object |
| Key | String | Object's key |
| LastModified | Date | Object's last-modified date/time |
| ETag | String | MD-5 hash of object (entity tag) |
| Size | Integer | Object's size in bytes |
| StorageClass | String | Always STANDARD |

---

## HTTP Status Codes

| Code | Status | Description |
|------|--------|-------------|
| 200 | OK | Request succeeded |
| 201 | Created | Resource created |
| 204 | No Content | Request succeeded, no content returned |
| 304 | Not Modified | Conditional request, resource not modified |
| 400 | Bad Request | Malformed request |
| 403 | Forbidden | Access denied |
| 404 | Not Found | Object/bucket not found |
| 409 | Conflict | Conflict (e.g., bucket already exists) |
| 412 | Precondition Failed | Conditional request failed |
| 416 | Range Not Satisfiable | Requested range not valid |
| 500 | Internal Server Error | Server error |

---

## Zen Garden Implementation Notes

### Scope (MVP)
For seed bank storage gateway, implement:
1. **PUT** - Store object to seed bank
2. **GET** - Retrieve object from seed bank
3. **HEAD** - Get object metadata
4. **DELETE** - Remove object from seed bank
5. **LIST** - List objects with prefix/delimiter support

### Path Mapping
```
S3 Path: /{bucket}/{key}
Local Path: {seed-bank-mount}/garden/storage/{bucket}/{key}
```

### Seed Bank Selection
- Optional header: `X-Seed-Bank: <name>`
- Optional query param: `seed-bank=<name>`
- Default seed bank name: `seed-bank-zen-garden`

### Application Namespacing (Client Convention)
- No server-side app isolation.
- Clients may prefix keys with `{app}/{bucket}/...` by convention.

### ETag Generation
- Calculate MD5 hash during upload
- Store as extended attribute or in metadata file
- Return in GET/HEAD/PUT responses

### No Auth (MVP)
- Skip signature validation for MVP
- Add AWS Signature V4 in later iteration

### Content-Type
- Store as extended attribute or metadata
- Default: `application/octet-stream`
