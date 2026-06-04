# API Documentation

Base URL: `http://localhost:8080`

All endpoints (except `POST /businesses`) require authentication:

```
Authorization: Bearer dodo_live_<prefix>.<secret>
```

---

## Error Format

All errors return a plain-text body with the appropriate HTTP status code.

```
HTTP/1.1 422 Unprocessable Entity

Invoice not found or is not in the Open state
```

| Status | Meaning |
|---|---|
| `400` | Bad request — missing or invalid fields |
| `401` | Missing or invalid API key |
| `404` | Resource not found |
| `409` | Conflict — idempotency key already in use |
| `422` | Unprocessable — invalid state transition |
| `500` | Internal server error |

---

## Businesses

### POST /businesses

Creates a new business and returns a one-time API key. No authentication required.

**Request**
```json
{
  "name": "Acme Corp",
  "email": "billing@acme.com"
}
```

**Response `201 Created`**
```json
{
  "business": {
    "id": "b75d8a32-f485-4f8d-a76b-b9421653a055",
    "name": "Acme Corp",
    "email": "billing@acme.com",
    "is_active": true,
    "created_at": "2026-06-04T10:00:00Z",
    "updated_at": "2026-06-04T10:00:00Z"
  },
  "api_key": "dodo_live_a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6.mysecret123"
}
```

> **Save the `api_key` value.** It is shown exactly once and never retrievable again. The raw key is never stored on the server.

---

## Customers

All customer endpoints require `Authorization` header.

### POST /customers

Creates a customer scoped to the authenticated business.

**Request**
```json
{
  "name": "Jane Smith",
  "email": "jane@example.com",
  "phone": "+1-555-0100"
}
```

`phone` is optional.

**Response `201 Created`**
```json
{
  "id": "c727edcd-d6fa-476b-8077-5b53c3bdb0dd",
  "business_id": "b75d8a32-f485-4f8d-a76b-b9421653a055",
  "name": "Jane Smith",
  "email": "jane@example.com",
  "phone": "+1-555-0100",
  "created_at": "2026-06-04T10:01:00Z",
  "updated_at": "2026-06-04T10:01:00Z"
}
```

**Errors**
- `400` — email already registered for this business

---

### GET /customers

Returns all customers for the authenticated business.

**Response `200 OK`**
```json
{
  "customers": [
    {
      "id": "c727edcd-d6fa-476b-8077-5b53c3bdb0dd",
      "business_id": "b75d8a32-f485-4f8d-a76b-b9421653a055",
      "name": "Jane Smith",
      "email": "jane@example.com",
      "phone": "+1-555-0100",
      "created_at": "2026-06-04T10:01:00Z",
      "updated_at": "2026-06-04T10:01:00Z"
    }
  ]
}
```

---

### GET /customers/{id}

Returns a single customer by ID.

**Response `200 OK`** — same shape as a single customer object above.

**Errors**
- `404` — customer not found or belongs to a different business

---

## Invoices

All invoice endpoints require `Authorization` header.

### POST /invoices

Creates an invoice in `Draft` state. Call `POST /invoices/{id}/finalize` to move it to `Open` and make it payable.

**Request**
```json
{
  "customer_id": "c727edcd-d6fa-476b-8077-5b53c3bdb0dd",
  "due_date": "2026-07-04T00:00:00Z",
  "line_items": [
    {
      "description": "Pro Plan - June 2026",
      "quantity": 1,
      "unit_amount_cents": 4900
    },
    {
      "description": "Extra Seats (x5)",
      "quantity": 5,
      "unit_amount_cents": 500
    }
  ]
}
```

`total_amount_cents` is computed server-side from line items. Do not send it.

**Response `201 Created`**
```json
{
  "invoice": {
    "id": "8d452e0e-00c3-4cb2-9a44-09a9f256a57a",
    "business_id": "b75d8a32-f485-4f8d-a76b-b9421653a055",
    "customer_id": "c727edcd-d6fa-476b-8077-5b53c3bdb0dd",
    "state": "Draft",
    "total_amount_cents": 7400,
    "due_date": "2026-07-04T00:00:00Z",
    "paid_at": null,
    "created_at": "2026-06-04T10:02:00Z",
    "updated_at": "2026-06-04T10:02:00Z"
  },
  "line_items": [
    {
      "id": "a1b2c3d4-...",
      "invoice_id": "8d452e0e-...",
      "description": "Pro Plan - June 2026",
      "quantity": 1,
      "unit_amount_cents": 4900,
      "created_at": "2026-06-04T10:02:00Z",
      "updated_at": "2026-06-04T10:02:00Z"
    },
    {
      "id": "b2c3d4e5-...",
      "invoice_id": "8d452e0e-...",
      "description": "Extra Seats (x5)",
      "quantity": 5,
      "unit_amount_cents": 500,
      "created_at": "2026-06-04T10:02:00Z",
      "updated_at": "2026-06-04T10:02:00Z"
    }
  ]
}
```

**Errors**
- `400` — invalid `customer_id`, empty `line_items`, invalid quantity or amount

---

### GET /invoices

Returns all invoices for the authenticated business. Optionally filter by state.

**Query Parameters**

| Param | Values | Example |
|---|---|---|
| `state` | `Draft`, `Open`, `Processing`, `Paid`, `Void`, `Uncollectible` | `?state=Open` |

**Response `200 OK`**
```json
[
  {
    "invoice": { ... },
    "line_items": [ ... ]
  }
]
```

---

### GET /invoices/{id}

Returns a single invoice with its line items.

**Response `200 OK`** — same shape as one element from the list above.

**Errors**
- `404` — invoice not found or belongs to a different business

---

### POST /invoices/{id}/finalize

Moves an invoice from `Draft` → `Open`, making it payable.

**Request** — empty body.

**Response `200 OK`** — the updated invoice object.

**Errors**
- `422` — invoice is not in `Draft` state
- `404` — invoice not found

---

### POST /invoices/{id}/pay

Charges the invoice via the PSP. Requires an `Idempotency-Key` header.

**Headers**
```
Idempotency-Key: pay-<invoice_id>-<attempt-uuid>
```

**Request**
```json
{
  "card_token": "tok_success"
}
```

**Mock PSP card tokens**

| Token | Result |
|---|---|
| `tok_success` | Payment succeeds → invoice moves to `Paid` |
| `tok_insufficient_funds` | Declined → invoice returns to `Open` |
| `tok_card_declined` | Declined → invoice returns to `Open` |
| `tok_timeout` | PSP hangs 30s, billing times out at 10s → `Open` |
| `tok_network_error` | PSP returns 500 → invoice returns to `Open` |

**Response `200 OK` — success**
```json
{
  "invoice": {
    "id": "8d452e0e-00c3-4cb2-9a44-09a9f256a57a",
    "state": "Paid",
    "paid_at": "2026-06-04T10:05:00Z",
    ...
  },
  "payment_attempt": {
    "id": "47ed1f8f-6218-46b0-960a-a3ced9b8e558",
    "invoice_id": "8d452e0e-00c3-4cb2-9a44-09a9f256a57a",
    "status": "Succeeded",
    "amount_cents": 7400,
    "card_token": "tok_success",
    "psp_ref": "psp_cc6acf07-09a4-4a7a-a8d0-2e5f5cc836ad",
    "failure_code": null,
    "created_at": "2026-06-04T10:05:00Z",
    "updated_at": "2026-06-04T10:05:00Z"
  }
}
```

**Response `402 Payment Required` — failure**
```json
{
  "invoice": {
    "id": "8d452e0e-...",
    "state": "Open",
    ...
  },
  "payment_attempt": {
    "status": "Failed",
    "failure_code": "insufficient_funds",
    ...
  }
}
```

**Errors**
- `400` — missing `Idempotency-Key` header
- `409` — same `Idempotency-Key` is already in flight
- `422` — invoice is not in `Open` state (already `Paid`, `Void`, etc.)

---

### POST /invoices/{id}/void

Moves an `Open` invoice to `Void` (terminal). No payment can be taken after this.

**Request** — empty body.

**Response `200 OK`** — the updated invoice object (`state: "Void"`).

**Errors**
- `422` — invoice is not in `Open` state
- `404` — invoice not found

---

### POST /invoices/{id}/mark_uncollectible

Moves an `Open` invoice to `Uncollectible` (terminal). Used for write-offs.

**Request** — empty body.

**Response `200 OK`** — the updated invoice object (`state: "Uncollectible"`).

**Errors**
- `422` — invoice is not in `Open` state
- `404` — invoice not found

---

## Webhooks

All webhook endpoints require `Authorization` header.

### POST /webhooks

Registers a new webhook endpoint for the authenticated business. The signing secret is shown **once only** in the response.

**Request**
```json
{
  "url": "https://your-server.com/webhooks/dodo"
}
```

**Response `201 Created`**
```json
{
  "id": "e3f4a5b6-...",
  "business_id": "b75d8a32-...",
  "url": "https://your-server.com/webhooks/dodo",
  "is_active": true,
  "secret": "a3f8c2d1e9b74056..."
}
```

> Store `secret` securely. It is used to verify the `X-Dodo-Signature` HMAC on incoming webhook deliveries. It will not be returned again.

**Events delivered**

| Event | Trigger |
|---|---|
| `invoice.created` | Invoice created |
| `invoice.paid` | Payment succeeded |
| `invoice.payment_failed` | Payment failed or timed out |

---

### GET /webhooks

Lists all active webhook endpoints for the authenticated business. The `secret` is never returned after creation.

**Response `200 OK`**
```json
[
  {
    "id": "e3f4a5b6-...",
    "business_id": "b75d8a32-...",
    "url": "https://your-server.com/webhooks/dodo",
    "is_active": true,
    "created_at": "2026-06-04T10:06:00Z"
  }
]
```

---

### DELETE /webhooks/{id}

Deactivates a webhook endpoint. It will stop receiving deliveries immediately.

**Response `200 OK`** — the deactivated endpoint (`is_active: false`).

**Errors**
- `404` — webhook not found or belongs to a different business
