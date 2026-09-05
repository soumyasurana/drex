# API Design

## Overview

The Contextra API provides a consistent interface for interacting with the platform.

The API is designed around a small set of principles:

- Predictable
- Consistent
- Versioned
- Observable
- Provider Independent

All public APIs follow the same conventions regardless of the underlying implementation.

---

# API Architecture

```text
                Client

                   │

                   ▼

            Gateway Service

                   │

        Authentication

                   │

        Request Validation

                   │

        Domain Libraries

                   │

              Response
```

The Gateway is the only public entry point.

Clients never communicate directly with internal libraries.

---

# API Style

Contextra exposes a REST API for external clients.

Responses use JSON.

Future versions may additionally expose:

- gRPC
- WebSockets
- Server-Sent Events

REST remains the primary public interface.

---

# Versioning

Every public endpoint is versioned.

Example:

```text
/api/v1/
```

Breaking changes require a new API version.

Minor improvements should remain backwards compatible.

---

# Authentication

Authentication is performed before requests reach domain libraries.

Supported authentication methods include:

- API Keys
- JWT
- OAuth 2.0 (future)
- Service Accounts (future)

Authentication is handled entirely by the Gateway.

---

# Authorization

Authorization determines whether an authenticated client may perform an action.

Examples include:

- Read documents
- Upload files
- Delete collections
- Execute workflows

Authorization policies remain independent from business logic.

---

# Request Validation

Every request is validated before execution.

Validation includes:

- Required fields
- Data types
- Constraints
- Payload size
- Authentication
- Authorization

Invalid requests never reach domain libraries.

---

# Resource Design

Resources use plural nouns.

Examples:

```text
/documents

/collections

/conversations

/messages

/prompts

/workflows
```

Resource names remain stable across API versions.

---

# HTTP Methods

Standard HTTP semantics are used.

| Method | Purpose |
|---------|----------|
| GET | Retrieve resources |
| POST | Create resources |
| PUT | Replace resources |
| PATCH | Partial updates |
| DELETE | Remove resources |

Methods should remain idempotent whenever possible.

---

# Status Codes

The API uses standard HTTP status codes.

Examples:

| Code | Meaning |
|------|----------|
| 200 | Success |
| 201 | Resource Created |
| 202 | Accepted |
| 204 | No Content |
| 400 | Bad Request |
| 401 | Unauthorized |
| 403 | Forbidden |
| 404 | Not Found |
| 409 | Conflict |
| 422 | Validation Error |
| 429 | Rate Limited |
| 500 | Internal Error |

Custom status codes are never introduced.

---

# Error Responses

All errors follow a common structure.

Example:

```json
{
  "error": {
    "code": "DOCUMENT_NOT_FOUND",
    "message": "Document not found.",
    "request_id": "..."
  }
}
```

Error responses should be predictable across every endpoint.

---

# Pagination

Large collections use cursor-based pagination.

Example:

```text
GET /documents?cursor=abc123&limit=25
```

Cursor pagination is preferred over offsets for scalability.

---

# Filtering

Filtering uses query parameters.

Example:

```text
/documents?collection=engineering

/documents?tag=rust

/documents?owner=user123
```

Filters should be composable.

---

# Sorting

Sorting follows a consistent convention.

Example:

```text
?sort=created_at

?sort=-updated_at
```

A leading minus indicates descending order.

---

# Streaming

Long-running operations may stream results.

Examples include:

- Chat completion
- Agent execution
- Document ingestion progress

Streaming support may use:

- Server-Sent Events
- WebSockets
- HTTP chunked responses

---

# Idempotency

Operations that create resources may support idempotency keys.

Example:

```text
Idempotency-Key:
```

Repeated requests with the same key should not create duplicate resources.

---

# Rate Limiting

Rate limiting protects the platform.

Limits may vary by:

- User
- API Key
- Organization
- Endpoint

Rate limit information is returned through standard HTTP headers.

---

# Request IDs

Every request receives a unique identifier.

Example:

```text
X-Request-ID
```

Request IDs simplify debugging and tracing.

---

# Observability

Every request generates:

- Structured logs
- Tracing spans
- Metrics

Examples include:

- Request duration
- Response size
- Status code
- Route
- Authentication method

---

# OpenAPI

Every public endpoint is documented.

Generated documentation includes:

- Schemas
- Parameters
- Authentication
- Examples

Interactive documentation is available through Swagger UI.

---

# API Principles

## Predictable

Identical requests should produce predictable responses.

---

## Consistent

Naming conventions remain uniform throughout the API.

---

## Versioned

Breaking changes require a new version.

---

## Observable

Every request should be traceable.

---

## Provider Independent

Clients never know which LLM provider is being used.

---

## Stable

Public APIs should remain stable over time.

---

# Future Capabilities

The API is designed to support future features including:

- GraphQL
- gRPC Gateway
- WebSocket subscriptions
- Workflow APIs
- Multi-tenant APIs
- Agent APIs
- Event subscriptions
- Batch operations

These capabilities can be added without redesigning the existing REST interface.

---

# Summary

The Contextra API provides a stable, versioned, and observable interface to the platform.

By enforcing consistent resource design, standardized error handling, and strong API conventions, the Gateway remains predictable for clients while allowing the underlying architecture to evolve independently.