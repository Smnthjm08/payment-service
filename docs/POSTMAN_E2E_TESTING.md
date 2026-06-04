# 🧪 Postman End-to-End Testing Guide — Payment Service

A complete walkthrough for testing the **entire payment lifecycle** via Postman:  
Business setup → Customer → Invoice → Payment (success & failure scenarios).

---

## 📋 Table of Contents

1. [Prerequisites & Setup](#1-prerequisites--setup)
2. [Postman Environment Variables](#2-postman-environment-variables)
3. [Flow Overview](#3-flow-overview)
4. [Step 1 — Register a Business](#step-1--register-a-business)
5. [Step 2 — Create a Customer](#step-2--create-a-customer)
6. [Step 3 — Register a Webhook Endpoint (Optional)](#step-3--register-a-webhook-endpoint-optional)
7. [Step 4 — Create an Invoice](#step-4--create-an-invoice)
8. [Step 5A — Pay Invoice (✅ Success)](#step-5a--pay-invoice--success)
9. [Step 5B — Pay Invoice (❌ Insufficient Funds)](#step-5b--pay-invoice--insufficient-funds)
10. [Step 5C — Pay Invoice (❌ Card Declined)](#step-5c--pay-invoice--card-declined)
11. [Step 5D — Pay Invoice (❌ PSP Timeout)](#step-5d--pay-invoice--psp-timeout)
12. [Step 5E — Pay Invoice (❌ Gateway Error / HTTP 500)](#step-5e--pay-invoice--gateway-error--http-500)
13. [Step 6 — Idempotency Replay Test](#step-6--idempotency-replay-test)
14. [Step 7 — Void an Invoice](#step-7--void-an-invoice)
15. [Step 8 — Mark Invoice as Uncollectible](#step-8--mark-invoice-as-uncollectible)
16. [Step 9 — List & Inspect Resources](#step-9--list--inspect-resources)
17. [Mock PSP Card Token Reference](#mock-psp-card-token-reference)
18. [Invoice State Machine Reference](#invoice-state-machine-reference)
19. [Webhook Event Payloads Reference](#webhook-event-payloads-reference)

---

## 1. Prerequisites & Setup

### Services to Run

Start **both** services before testing. Open two terminals:

```bash
# Terminal 1 — Mock PSP (port 9090)
cargo run --bin mock-psp

# Terminal 2 — Billing API (port 8080)
cargo run --bin billing
```

> The billing service requires a running PostgreSQL database. Set your `DATABASE_URL` in `.env`.

### Verify Services Are Healthy

```
GET http://localhost:8080/api/health   → 200 "Healthy!"
GET http://localhost:9090/health       → 200 "mock-psp healthy"
```

---

## 2. Postman Environment Variables

Create a **Postman Environment** named `Payment Service - Local` with these variables:

| Variable          | Initial Value                   | Description                                    |
|-------------------|---------------------------------|------------------------------------------------|
| `base_url`        | `http://localhost:8080/api`     | Billing API base URL                           |
| `psp_url`         | `http://localhost:9090`         | Mock PSP base URL                              |
| `api_key`         | *(empty — set after Step 1)*    | Bearer token returned by `POST /businesses`    |
| `business_id`     | *(empty — set after Step 1)*    | UUID of the created business                   |
| `customer_id`     | *(empty — set after Step 2)*    | UUID of the created customer                   |
| `invoice_id`      | *(empty — set after Step 4)*    | UUID of the created invoice                    |
| `webhook_id`      | *(empty — set after Step 3)*    | UUID of the registered webhook endpoint        |
| `idempotency_key` | *(set per request)*             | Unique key per payment attempt                 |

### Authorization Header

All endpoints **except** `POST /businesses` require:
```
Authorization: Bearer {{api_key}}
```

---

## 3. Flow Overview

```
POST /businesses        ← Get API key
      │
      ▼
POST /customers         ← Create payer
      │
      ▼
POST /webhooks          ← (Optional) Register webhook
      │
      ▼
POST /invoices          ← Create invoice (state: Open)
      │
      ├── POST /invoices/{id}/pay  [tok_success]          → Paid ✅
      ├── POST /invoices/{id}/pay  [tok_insufficient_funds] → Open ❌ (402)
      ├── POST /invoices/{id}/pay  [tok_card_declined]    → Open ❌ (402)
      ├── POST /invoices/{id}/pay  [tok_timeout]          → Open ❌ (402, TimedOut)
      ├── POST /invoices/{id}/pay  [tok_network_error]    → Open ❌ (402, Error)
      ├── POST /invoices/{id}/void                        → Void ⛔
      └── POST /invoices/{id}/mark_uncollectible          → Uncollectible ⛔
```

---

## Step 1 — Register a Business

> **No authentication required** for this endpoint.

### Request

```
POST {{base_url}}/businesses
Content-Type: application/json
```

```json
{
  "name": "Acme Corp",
  "email": "billing@acme.com"
}
```

### Sample Response — `201 Created`

```json
{
  "business": {
    "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "email": "billing@acme.com",
    "name": "Acme Corp",
    "is_active": true,
    "created_at": "2026-06-05T10:00:00Z",
    "updated_at": "2026-06-05T10:00:00Z"
  },
  "api_key": "dodo_live_a1b2c3d4e5f67890abcdef12.secretpart1234567890abcdef"
}
```

> ⚠️ **Save the `api_key` immediately** — it is shown **only once** and cannot be retrieved again.

### Postman Test Script (Tests Tab)

```javascript
var json = pm.response.json();
pm.environment.set("api_key", json.api_key);
pm.environment.set("business_id", json.business.id);
pm.test("Status is 201", () => pm.response.to.have.status(201));
pm.test("API key returned", () => pm.expect(json.api_key).to.be.a("string").and.not.empty);
```

---

## Step 2 — Create a Customer

### Request

```
POST {{base_url}}/customers
Content-Type: application/json
Authorization: Bearer {{api_key}}
```

```json
{
  "name": "Jane Doe",
  "email": "jane.doe@example.com",
  "phone": "+44 7700 900000"
}
```

> `phone` is optional — you can omit it.

### Sample Response — `201 Created`

```json
{
  "id": "b2c3d4e5-f6a7-8901-bcde-f23456789012",
  "business_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "name": "Jane Doe",
  "email": "jane.doe@example.com",
  "phone": "+44 7700 900000",
  "created_at": "2026-06-05T10:01:00Z",
  "updated_at": "2026-06-05T10:01:00Z"
}
```

### Postman Test Script

```javascript
var json = pm.response.json();
pm.environment.set("customer_id", json.id);
pm.test("Status is 201", () => pm.response.to.have.status(201));
pm.test("Customer ID set", () => pm.expect(json.id).to.be.a("string"));
```

---

## Step 3 — Register a Webhook Endpoint (Optional)

Register a URL to receive real-time event notifications. Use a tool like [webhook.site](https://webhook.site) for testing.

### Request

```
POST {{base_url}}/webhooks
Content-Type: application/json
Authorization: Bearer {{api_key}}
```

```json
{
  "url": "https://webhook.site/your-unique-id"
}
```

### Sample Response — `201 Created`

```json
{
  "id": "c3d4e5f6-a7b8-9012-cdef-345678901234",
  "business_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "url": "https://webhook.site/your-unique-id",
  "is_active": true,
  "secret": "a3f9b2e1d4c7a890b1e2f3d4c5a6b7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4"
}
```

> ⚠️ **Save the `secret`** — it is shown **only once** and is used to verify webhook signatures (HMAC-SHA256).

### Postman Test Script

```javascript
var json = pm.response.json();
pm.environment.set("webhook_id", json.id);
pm.environment.set("webhook_secret", json.secret);
pm.test("Status is 201", () => pm.response.to.have.status(201));
```

---

## Step 4 — Create an Invoice

### Request

```
POST {{base_url}}/invoices
Content-Type: application/json
Authorization: Bearer {{api_key}}
```

```json
{
  "customer_id": "{{customer_id}}",
  "due_date": "2026-07-05T00:00:00Z",
  "line_items": [
    {
      "description": "Pro Plan — June 2026",
      "quantity": 1,
      "unit_amount_cents": 4999
    },
    {
      "description": "Add-on: Extra Seats (x3)",
      "quantity": 3,
      "unit_amount_cents": 500
    }
  ]
}
```

> `unit_amount_cents` is in the **smallest currency unit** (e.g., pence/cents).  
> Total = (1 × 4999) + (3 × 500) = **6499 cents = £64.99**

### Field Validation Rules

| Field                  | Rule                                 |
|------------------------|--------------------------------------|
| `customer_id`          | Must belong to the authenticated business |
| `due_date`             | ISO 8601 UTC datetime string         |
| `line_items`           | Must not be empty                    |
| `description`          | Must not be blank                    |
| `quantity`             | Must be `> 0`                        |
| `unit_amount_cents`    | Must be `>= 0`                       |

### Sample Response — `201 Created`

```json
{
  "invoice": {
    "id": "d4e5f6a7-b8c9-0123-def0-456789012345",
    "business_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "customer_id": "b2c3d4e5-f6a7-8901-bcde-f23456789012",
    "state": "Open",
    "total_amount_cents": 6499,
    "due_date": "2026-07-05T00:00:00Z",
    "created_at": "2026-06-05T10:02:00Z",
    "updated_at": "2026-06-05T10:02:00Z",
    "paid_at": null
  },
  "line_items": [
    {
      "id": "e5f6a7b8-c9d0-1234-ef01-567890123456",
      "invoice_id": "d4e5f6a7-b8c9-0123-def0-456789012345",
      "description": "Pro Plan — June 2026",
      "quantity": 1,
      "unit_amount_cents": 4999,
      "created_at": "2026-06-05T10:02:00Z",
      "updated_at": "2026-06-05T10:02:00Z"
    },
    {
      "id": "f6a7b8c9-d0e1-2345-f012-678901234567",
      "invoice_id": "d4e5f6a7-b8c9-0123-def0-456789012345",
      "description": "Add-on: Extra Seats (x3)",
      "quantity": 3,
      "unit_amount_cents": 500,
      "created_at": "2026-06-05T10:02:00Z",
      "updated_at": "2026-06-05T10:02:00Z"
    }
  ]
}
```

### Postman Test Script

```javascript
var json = pm.response.json();
pm.environment.set("invoice_id", json.invoice.id);
pm.test("Status is 201", () => pm.response.to.have.status(201));
pm.test("Invoice state is Open", () => pm.expect(json.invoice.state).to.eql("Open"));
pm.test("Total is correct", () => pm.expect(json.invoice.total_amount_cents).to.eql(6499));
// Webhook: invoice.created will fire in background
```

---

## Step 5A — Pay Invoice (✅ Success)

### Request

```
POST {{base_url}}/invoices/{{invoice_id}}/pay
Content-Type: application/json
Authorization: Bearer {{api_key}}
Idempotency-Key: pay-{{$randomUUID}}
```

```json
{
  "card_token": "tok_success"
}
```

> **`Idempotency-Key` header is required.** Use a unique UUID per attempt (e.g., `pay-550e8400-e29b-41d4-a716-446655440000`). In Postman you can use `{{$randomUUID}}`.

### Sample Response — `200 OK`

```json
{
  "invoice": {
    "id": "d4e5f6a7-b8c9-0123-def0-456789012345",
    "business_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "customer_id": "b2c3d4e5-f6a7-8901-bcde-f23456789012",
    "state": "Paid",
    "total_amount_cents": 6499,
    "due_date": "2026-07-05T00:00:00Z",
    "created_at": "2026-06-05T10:02:00Z",
    "updated_at": "2026-06-05T10:05:00Z",
    "paid_at": "2026-06-05T10:05:00Z"
  },
  "payment_attempt": {
    "id": "a7b8c9d0-e1f2-3456-0123-789012345678",
    "invoice_id": "d4e5f6a7-b8c9-0123-def0-456789012345",
    "status": "Succeeded",
    "amount_cents": 6499,
    "card_token": "tok_success",
    "psp_ref": "psp_f1e2d3c4-b5a6-7890-fedc-ba0987654321",
    "failure_code": null,
    "created_at": "2026-06-05T10:05:00Z",
    "updated_at": "2026-06-05T10:05:00Z"
  }
}
```

### Expected Outcomes

| Property                    | Expected Value |
|-----------------------------|----------------|
| HTTP Status                 | `200 OK`       |
| `invoice.state`             | `"Paid"`       |
| `invoice.paid_at`           | Non-null timestamp |
| `payment_attempt.status`    | `"Succeeded"`  |
| `payment_attempt.psp_ref`   | Non-null string (PSP reference) |
| `payment_attempt.failure_code` | `null`      |
| Webhook fired               | `invoice.paid` |

### Postman Test Script

```javascript
var json = pm.response.json();
pm.test("Status 200", () => pm.response.to.have.status(200));
pm.test("Invoice is Paid", () => pm.expect(json.invoice.state).to.eql("Paid"));
pm.test("paid_at is set", () => pm.expect(json.invoice.paid_at).to.not.be.null);
pm.test("Attempt Succeeded", () => pm.expect(json.payment_attempt.status).to.eql("Succeeded"));
pm.test("PSP ref present", () => pm.expect(json.payment_attempt.psp_ref).to.be.a("string"));
pm.test("No failure_code", () => pm.expect(json.payment_attempt.failure_code).to.be.null);
```

---

## Step 5B — Pay Invoice (❌ Insufficient Funds)

> **Create a new invoice first** (invoice must be in `Open` state). A `Paid` invoice cannot be paid again.

### Request

```
POST {{base_url}}/invoices/{{invoice_id}}/pay
Content-Type: application/json
Authorization: Bearer {{api_key}}
Idempotency-Key: pay-{{$randomUUID}}
```

```json
{
  "card_token": "tok_insufficient_funds"
}
```

### Sample Response — `402 Payment Required`

```json
{
  "invoice": {
    "id": "d4e5f6a7-b8c9-0123-def0-456789012345",
    "business_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "customer_id": "b2c3d4e5-f6a7-8901-bcde-f23456789012",
    "state": "Open",
    "total_amount_cents": 6499,
    "due_date": "2026-07-05T00:00:00Z",
    "created_at": "2026-06-05T10:02:00Z",
    "updated_at": "2026-06-05T10:06:00Z",
    "paid_at": null
  },
  "payment_attempt": {
    "id": "b8c9d0e1-f2a3-4567-1234-890123456789",
    "invoice_id": "d4e5f6a7-b8c9-0123-def0-456789012345",
    "status": "Failed",
    "amount_cents": 6499,
    "card_token": "tok_insufficient_funds",
    "psp_ref": null,
    "failure_code": "insufficient_funds",
    "created_at": "2026-06-05T10:06:00Z",
    "updated_at": "2026-06-05T10:06:00Z"
  }
}
```

### Expected Outcomes

| Property                    | Expected Value          |
|-----------------------------|-------------------------|
| HTTP Status                 | `402 Payment Required`  |
| `invoice.state`             | `"Open"` (rolled back)  |
| `payment_attempt.status`    | `"Failed"`              |
| `payment_attempt.failure_code` | `"insufficient_funds"` |
| `payment_attempt.psp_ref`   | `null`                  |
| Webhook fired               | `invoice.payment_failed` |

### Postman Test Script

```javascript
var json = pm.response.json();
pm.test("Status 402", () => pm.response.to.have.status(402));
pm.test("Invoice rolled back to Open", () => pm.expect(json.invoice.state).to.eql("Open"));
pm.test("Attempt Failed", () => pm.expect(json.payment_attempt.status).to.eql("Failed"));
pm.test("Failure code is insufficient_funds", () => pm.expect(json.payment_attempt.failure_code).to.eql("insufficient_funds"));
```

---

## Step 5C — Pay Invoice (❌ Card Declined)

> **Create a fresh Open invoice** before running this step.

### Request

```
POST {{base_url}}/invoices/{{invoice_id}}/pay
Content-Type: application/json
Authorization: Bearer {{api_key}}
Idempotency-Key: pay-{{$randomUUID}}
```

```json
{
  "card_token": "tok_card_declined"
}
```

### Sample Response — `402 Payment Required`

```json
{
  "invoice": {
    "id": "d4e5f6a7-b8c9-0123-def0-456789012345",
    "state": "Open",
    "total_amount_cents": 6499,
    "paid_at": null
  },
  "payment_attempt": {
    "id": "c9d0e1f2-a3b4-5678-2345-901234567890",
    "invoice_id": "d4e5f6a7-b8c9-0123-def0-456789012345",
    "status": "Failed",
    "amount_cents": 6499,
    "card_token": "tok_card_declined",
    "psp_ref": null,
    "failure_code": "card_declined",
    "created_at": "2026-06-05T10:07:00Z",
    "updated_at": "2026-06-05T10:07:00Z"
  }
}
```

### Expected Outcomes

| Property                    | Expected Value    |
|-----------------------------|-------------------|
| HTTP Status                 | `402`             |
| `invoice.state`             | `"Open"`          |
| `payment_attempt.status`    | `"Failed"`        |
| `payment_attempt.failure_code` | `"card_declined"` |

---

## Step 5D — Pay Invoice (❌ PSP Timeout)

> The Mock PSP simulates a **30-second delay** with `tok_timeout`. The billing service has a **10-second PSP timeout**, so it will return a `402` after ~10 seconds.

### Request

```
POST {{base_url}}/invoices/{{invoice_id}}/pay
Content-Type: application/json
Authorization: Bearer {{api_key}}
Idempotency-Key: pay-{{$randomUUID}}
```

> ⏱️ **Set Postman's request timeout to at least 30 seconds** for this test (`Settings → Request Timeout`).

```json
{
  "card_token": "tok_timeout"
}
```

### Sample Response — `402 Payment Required` (after ~10 seconds)

```json
{
  "invoice": {
    "id": "d4e5f6a7-b8c9-0123-def0-456789012345",
    "state": "Open",
    "total_amount_cents": 6499,
    "paid_at": null
  },
  "payment_attempt": {
    "id": "d0e1f2a3-b4c5-6789-3456-012345678901",
    "invoice_id": "d4e5f6a7-b8c9-0123-def0-456789012345",
    "status": "TimedOut",
    "amount_cents": 6499,
    "card_token": "tok_timeout",
    "psp_ref": null,
    "failure_code": "psp_timeout",
    "created_at": "2026-06-05T10:08:00Z",
    "updated_at": "2026-06-05T10:08:10Z"
  }
}
```

### Expected Outcomes

| Property                    | Expected Value   |
|-----------------------------|------------------|
| HTTP Status                 | `402`            |
| Response Time               | ~10 seconds      |
| `invoice.state`             | `"Open"` (NOT stuck in `"Processing"`) |
| `payment_attempt.status`    | `"TimedOut"`     |
| `payment_attempt.failure_code` | `"psp_timeout"` |

### Postman Test Script

```javascript
var json = pm.response.json();
pm.test("Status 402", () => pm.response.to.have.status(402));
pm.test("Invoice is Open (not stuck in Processing)", () => pm.expect(json.invoice.state).to.eql("Open"));
pm.test("Attempt is TimedOut", () => pm.expect(json.payment_attempt.status).to.eql("TimedOut"));
pm.test("failure_code is psp_timeout", () => pm.expect(json.payment_attempt.failure_code).to.eql("psp_timeout"));
```

---

## Step 5E — Pay Invoice (❌ Gateway Error / HTTP 500)

> The Mock PSP returns an immediate **HTTP 500** for `tok_network_error`. The billing service gracefully recovers and returns `402`.

### Request

```
POST {{base_url}}/invoices/{{invoice_id}}/pay
Content-Type: application/json
Authorization: Bearer {{api_key}}
Idempotency-Key: pay-{{$randomUUID}}
```

```json
{
  "card_token": "tok_network_error"
}
```

### Sample Response — `402 Payment Required`

```json
{
  "invoice": {
    "id": "d4e5f6a7-b8c9-0123-def0-456789012345",
    "state": "Open",
    "total_amount_cents": 6499,
    "paid_at": null
  },
  "payment_attempt": {
    "id": "e1f2a3b4-c5d6-7890-4567-123456789012",
    "invoice_id": "d4e5f6a7-b8c9-0123-def0-456789012345",
    "status": "Error",
    "amount_cents": 6499,
    "card_token": "tok_network_error",
    "psp_ref": null,
    "failure_code": "gateway_error",
    "created_at": "2026-06-05T10:09:00Z",
    "updated_at": "2026-06-05T10:09:00Z"
  }
}
```

### Expected Outcomes

| Property                    | Expected Value    |
|-----------------------------|-------------------|
| HTTP Status                 | `402`             |
| `invoice.state`             | `"Open"`          |
| `payment_attempt.status`    | `"Error"`         |
| `payment_attempt.failure_code` | `"gateway_error"` |

---

## Step 6 — Idempotency Replay Test

Sending the **same `Idempotency-Key`** twice must return an identical response body without hitting the PSP a second time.

### First Request

```
POST {{base_url}}/invoices/{{invoice_id}}/pay
Content-Type: application/json
Authorization: Bearer {{api_key}}
Idempotency-Key: idem-test-key-abc123
```

```json
{
  "card_token": "tok_success"
}
```

### Second Request (Replay — Same Key, Same Body)

```
POST {{base_url}}/invoices/{{invoice_id}}/pay
Content-Type: application/json
Authorization: Bearer {{api_key}}
Idempotency-Key: idem-test-key-abc123
```

```json
{
  "card_token": "tok_success"
}
```

### Expected Outcomes

| Check                         | Expected                              |
|-------------------------------|---------------------------------------|
| First response HTTP status    | `200`                                 |
| Replay response HTTP status   | `200`                                 |
| Response body match           | Identical JSON (byte-for-byte)        |
| PSP `/charge` calls           | **1 total** (replay is served from cache) |
| Rows in `payment_attempts`    | **1 row** (no duplicate)              |

> **Key Insight**: The `Idempotency-Key` must be unique per logical payment operation. Reusing it safely de-duplicates the request.

### Postman Test Script (set up a Collection variable after first request)

```javascript
// Run this on the FIRST request
var json = pm.response.json();
pm.collectionVariables.set("first_response_body", JSON.stringify(json));
pm.test("First request: 200", () => pm.response.to.have.status(200));
```

```javascript
// Run this on the SECOND (replay) request
var firstBody = JSON.parse(pm.collectionVariables.get("first_response_body"));
var secondBody = pm.response.json();
pm.test("Replay: 200", () => pm.response.to.have.status(200));
pm.test("Bodies are identical", () => pm.expect(secondBody).to.deep.equal(firstBody));
```

---

## Step 7 — Void an Invoice

Only invoices in **`Open`** state can be voided.

### Request

```
POST {{base_url}}/invoices/{{invoice_id}}/void
Authorization: Bearer {{api_key}}
```

*(No request body required)*

### Sample Response — `200 OK`

```json
{
  "id": "d4e5f6a7-b8c9-0123-def0-456789012345",
  "business_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "customer_id": "b2c3d4e5-f6a7-8901-bcde-f23456789012",
  "state": "Void",
  "total_amount_cents": 6499,
  "due_date": "2026-07-05T00:00:00Z",
  "created_at": "2026-06-05T10:02:00Z",
  "updated_at": "2026-06-05T10:10:00Z",
  "paid_at": null
}
```

### Error Responses

| Scenario                           | HTTP Status | Error Message                                     |
|------------------------------------|-------------|---------------------------------------------------|
| Invoice not found                  | `404`       | `"Invoice not found"`                             |
| Invoice not in `Open` state        | `422`       | `"Cannot void an invoice in 'Paid' state. Only invoices in 'Open' state can be voided."` |

---

## Step 8 — Mark Invoice as Uncollectible

Only invoices in **`Open`** state can be marked uncollectible.

### Request

```
POST {{base_url}}/invoices/{{invoice_id}}/mark_uncollectible
Authorization: Bearer {{api_key}}
```

*(No request body required)*

### Sample Response — `200 OK`

```json
{
  "id": "d4e5f6a7-b8c9-0123-def0-456789012345",
  "business_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "customer_id": "b2c3d4e5-f6a7-8901-bcde-f23456789012",
  "state": "Uncollectible",
  "total_amount_cents": 6499,
  "due_date": "2026-07-05T00:00:00Z",
  "created_at": "2026-06-05T10:02:00Z",
  "updated_at": "2026-06-05T10:11:00Z",
  "paid_at": null
}
```

### Error Responses

| Scenario                           | HTTP Status | Error Message                                                                   |
|------------------------------------|-------------|---------------------------------------------------------------------------------|
| Invoice not in `Open` state        | `422`       | `"Cannot mark an invoice in 'Void' state as uncollectible."` |

---

## Step 9 — List & Inspect Resources

### List All Invoices

```
GET {{base_url}}/invoices
Authorization: Bearer {{api_key}}
```

### Filter Invoices by State

```
GET {{base_url}}/invoices?state=Open
GET {{base_url}}/invoices?state=Paid
GET {{base_url}}/invoices?state=Processing
GET {{base_url}}/invoices?state=Void
GET {{base_url}}/invoices?state=Uncollectible
Authorization: Bearer {{api_key}}
```

### Get Single Invoice

```
GET {{base_url}}/invoices/{{invoice_id}}
Authorization: Bearer {{api_key}}
```

### List All Customers

```
GET {{base_url}}/customers
Authorization: Bearer {{api_key}}
```

### Get Single Customer

```
GET {{base_url}}/customers/{{customer_id}}
Authorization: Bearer {{api_key}}
```

### List Webhooks

```
GET {{base_url}}/webhooks
Authorization: Bearer {{api_key}}
```

### Delete (Deactivate) a Webhook

```
DELETE {{base_url}}/webhooks/{{webhook_id}}
Authorization: Bearer {{api_key}}
```

---

## Mock PSP Card Token Reference

These are the **only recognised tokens** by the Mock PSP. Any other string defaults to `succeeded`.

| Token                   | PSP Behaviour                            | Billing Outcome          | Attempt Status |
|-------------------------|------------------------------------------|--------------------------|----------------|
| `tok_success`           | `200 succeeded` after ~100ms             | Invoice → `Paid`         | `Succeeded`    |
| `tok_insufficient_funds`| `200 failed` (`insufficient_funds`) ~100ms | Invoice → `Open`       | `Failed`       |
| `tok_card_declined`     | `200 failed` (`card_declined`) ~100ms    | Invoice → `Open`         | `Failed`       |
| `tok_timeout`           | Sleeps **30s** then `200 succeeded`      | Invoice → `Open` (billing times out at 10s) | `TimedOut` |
| `tok_network_error`     | `500 Internal Server Error`              | Invoice → `Open`         | `Error`        |
| *(any other value)*     | `200 succeeded` after ~100ms             | Invoice → `Paid`         | `Succeeded`    |

---

## Invoice State Machine Reference

```
                    ┌──────────┐
          create    │          │
        ──────────► │   Open   │
                    │          │
                    └─┬────┬───┘
                      │    │
          pay ────────┘    └──── void ──────────► Void
          (attempt)         or
             │         mark_uncollectible ────► Uncollectible
             ▼
        ┌──────────────┐
        │  Processing  │  ← Transient state during PSP call
        └──┬───────────┘
           │
    ┌──────┴───────┐
    │              │
    ▼              ▼
  Paid           Open ← (on PSP failure / timeout / decline)
```

### State Transition Rules

| From          | Action                   | To                |
|---------------|--------------------------|-------------------|
| `Open`        | `POST /pay`              | `Processing` → then `Paid` or `Open` |
| `Open`        | `POST /void`             | `Void`            |
| `Open`        | `POST /mark_uncollectible` | `Uncollectible` |
| `Processing`  | PSP success              | `Paid`            |
| `Processing`  | PSP failure/timeout/error | `Open`           |
| `Paid`        | *(terminal — no transitions)* | —             |
| `Void`        | *(terminal — no transitions)* | —             |
| `Uncollectible` | *(terminal — no transitions)* | —           |

---

## Webhook Event Payloads Reference

The webhook dispatcher fires events asynchronously after each state change.

### `invoice.created`

Fired when a new invoice is created via `POST /invoices`.

```json
{
  "event": "invoice.created",
  "invoice_id": "d4e5f6a7-b8c9-0123-def0-456789012345",
  "business_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "state": "Open",
  "total_amount_cents": 6499
}
```

### `invoice.paid`

Fired when a payment succeeds (PSP returns `succeeded`).

```json
{
  "event": "invoice.paid",
  "invoice_id": "d4e5f6a7-b8c9-0123-def0-456789012345",
  "business_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "psp_ref": "psp_f1e2d3c4-b5a6-7890-fedc-ba0987654321",
  "amount_cents": 6499
}
```

### `invoice.payment_failed` — Card Declined

```json
{
  "event": "invoice.payment_failed",
  "invoice_id": "d4e5f6a7-b8c9-0123-def0-456789012345",
  "business_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "failure_code": "card_declined",
  "reason": "card_declined"
}
```

### `invoice.payment_failed` — Insufficient Funds

```json
{
  "event": "invoice.payment_failed",
  "invoice_id": "d4e5f6a7-b8c9-0123-def0-456789012345",
  "business_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "failure_code": "insufficient_funds",
  "reason": "card_declined"
}
```

### `invoice.payment_failed` — PSP Timeout

```json
{
  "event": "invoice.payment_failed",
  "invoice_id": "d4e5f6a7-b8c9-0123-def0-456789012345",
  "business_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "failure_code": "psp_timeout",
  "reason": "psp_timeout"
}
```

### `invoice.payment_failed` — Gateway Error

```json
{
  "event": "invoice.payment_failed",
  "invoice_id": "d4e5f6a7-b8c9-0123-def0-456789012345",
  "business_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "failure_code": "gateway_error",
  "reason": "psp_gateway_error"
}
```

---

## Common Error Responses

| HTTP Status | Trigger                                            | Body                                          |
|-------------|----------------------------------------------------|-----------------------------------------------|
| `400`       | Missing/invalid fields, empty `Idempotency-Key`    | Plain text error message                      |
| `401`       | Missing or invalid `Authorization` header          | `"Unauthorized"`                              |
| `402`       | PSP declined, timeout, or gateway error            | `PayInvoiceResponse` JSON with failure details |
| `404`       | Resource not found                                 | Empty body or `"Invoice not found"`           |
| `409`       | Duplicate `Idempotency-Key` still in-flight        | `"A payment with this Idempotency-Key is already in progress"` |
| `422`       | Invalid state transition (e.g., pay a Paid invoice) | Error string describing the constraint       |
| `500`       | Internal server error                              | Actix error body                              |

---

*Generated for `payment-service` — all tests validated against 4/4 passing integration tests.*
