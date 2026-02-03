---
globs: src/moss/src/api/**/*.rs, src/rake/src/api/**/*.rs
alwaysApply: false
---
# API Handler Rules

## Response Types
- Success: `Json<T>` with appropriate status code
- Error: `(StatusCode, Json<ErrorResponse>)` with codes from `garden_common::constants`

## Error Handling
```rust
// Convert domain errors to API responses
match domain_result {
    Ok(data) => Ok(Json(data)),
    Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::from(e))))
}
```

## Logging
- Log at ERROR level for failures
- Include request context (job_id, resource_id, etc.)

## Endpoints Reference
See `docs/ARCHITECTURE-REFERENCE.md` for complete endpoint documentation.
