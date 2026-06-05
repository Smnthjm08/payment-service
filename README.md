# Payment Service

A multi-tenant billing API built with Rust (Actix Web), PostgreSQL, and SQLx. Supports API-key authentication, customer and invoice management, an enforced invoice state machine, idempotent PSP-backed payments, and signed async webhook delivery.

---

## Running the Service

### Docker (recommended — one command)

```bash
docker compose up --build
```

This starts:

1. **PostgreSQL** on port `5432`
2. **Mock PSP** on port `9090`
3. **Billing API** on port `8080` (migrations run automatically on startup)

To stop:

```bash
docker compose down       # keep DB volume
docker compose down -v    # wipe DB too
```

### Local / Native

```bash
# Copy environment config
cp .env.example .env

# Start mock PSP (separate terminal)
cargo run --bin mock-psp

# Start billing API
cargo run --bin billing
```

---

## Environment Variables

| Variable        | Default                 | Purpose                                 |
| --------------- | ----------------------- | --------------------------------------- |
| `DATABASE_URL`  | —                       | PostgreSQL connection string (required) |
| `PSP_URL`       | `http://localhost:9090` | Base URL of the mock PSP                |
| `PORT`          | `8080`                  | Billing API listen port                 |
| `MOCK_PSP_PORT` | `9090`                  | Mock PSP listen port                    |
| `RUST_LOG`      | —                       | Log level e.g. `billing=debug,info`     |

---

## Example Usage (curl)

The examples below walk through the full payment lifecycle. Replace `$API_KEY` with the key returned when you create a business.

### 1. Create a Business

No authentication required. Save the `api_key` — it is shown only once.

```bash
curl -s -X POST http://localhost:8080/businesses \
  -H "Content-Type: application/json" \
  -d '{"name": "Acme Corp", "email": "billing@acme.com"}' | jq .
```

```json
{
  "business": {
    "id": "b75d8a32-f485-4f8d-a76b-b9421653a055",
    "name": "Acme Corp",
    "email": "billing@acme.com",
    "is_active": true
  },
  "api_key": "dodo_live_abc123.secretxyz"
}
```

```bash
export API_KEY="dodo_live_abc123.secretxyz"
```

---

### 2. Create a Customer

```bash
curl -s -X POST http://localhost:8080/customers \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Jane Smith",
    "email": "jane@example.com",
    "phone": "+1-555-0100"
  }' | jq .
```

```json
{
  "id": "c727edcd-d6fa-476b-8077-5b53c3bdb0dd",
  "business_id": "b75d8a32-f485-4f8d-a76b-b9421653a055",
  "name": "Jane Smith",
  "email": "jane@example.com",
  "phone": "+1-555-0100"
}
```

```bash
export CUSTOMER_ID="c727edcd-d6fa-476b-8077-5b53c3bdb0dd"
```

---

### 3. Create an Invoice

Invoices start in `Draft` state. The server computes `total_amount_cents` from line items.

```bash
curl -s -X POST http://localhost:8080/invoices \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d "{
    \"customer_id\": \"$CUSTOMER_ID\",
    \"due_date\": \"2026-12-31T00:00:00Z\",
    \"line_items\": [
      { \"description\": \"Pro Plan\", \"quantity\": 1, \"unit_amount_cents\": 4900 },
      { \"description\": \"Extra Seats\", \"quantity\": 3, \"unit_amount_cents\": 500 }
    ]
  }" | jq .
```

```json
{
  "invoice": {
    "id": "8d452e0e-00c3-4cb2-9a44-09a9f256a57a",
    "state": "Draft",
    "total_amount_cents": 6400
  },
  "line_items": [...]
}
```

```bash
export INVOICE_ID="8d452e0e-00c3-4cb2-9a44-09a9f256a57a"
```

#### Finalize the Invoice (Draft → Open)

```bash
curl -s -X POST http://localhost:8080/invoices/$INVOICE_ID/finalize \
  -H "Authorization: Bearer $API_KEY" | jq .state
# "Open"
```

---

### 4. Pay the Invoice — Success

```bash
curl -s -X POST http://localhost:8080/invoices/$INVOICE_ID/pay \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: pay-$(uuidgen)" \
  -d '{"card_token": "tok_success"}' | jq '{state: .invoice.state, status: .payment_attempt.status}'
```

```json
{
  "state": "Paid",
  "status": "Succeeded"
}
```

---

### 5. Pay the Invoice — Failure (Insufficient Funds)

Create a fresh invoice first (a `Paid` invoice cannot be retried), then:

```bash
curl -s -X POST http://localhost:8080/invoices/$INVOICE_ID/pay \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: pay-$(uuidgen)" \
  -d '{"card_token": "tok_insufficient_funds"}' | jq '{state: .invoice.state, status: .payment_attempt.status, failure_code: .payment_attempt.failure_code}'
```

```json
{
  "state": "Open",
  "status": "Failed",
  "failure_code": "insufficient_funds"
}
```

The invoice returns to `Open` and can be retried with a different card token.

---

## Mock PSP Card Tokens

| Token                    | Result                                           |
| ------------------------ | ------------------------------------------------ |
| `tok_success`            | Succeeds → invoice moves to `Paid`               |
| `tok_insufficient_funds` | Declined → invoice returns to `Open`             |
| `tok_card_declined`      | Declined → invoice returns to `Open`             |
| `tok_timeout`            | PSP hangs 30s; billing times out at 10s → `Open` |
| `tok_network_error`      | PSP returns 500 → invoice returns to `Open`      |

---

## Integration Tests

```bash
# Requires a running Postgres instance
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres

cargo test --test payment_integration_tests -- --nocapture
```

| Test                                                  | Verifies                                                             |
| ----------------------------------------------------- | -------------------------------------------------------------------- |
| `test_concurrent_pay_only_one_succeeds`               | 10 concurrent `/pay` requests — exactly one succeeds, rest get `422` |
| `test_idempotent_pay_replays_without_second_psp_call` | Same `Idempotency-Key` replays cached response; PSP called once      |
| `test_psp_timeout_invoice_not_stuck_in_processing`    | Billing times out at 10s; invoice rolls back to `Open`               |
| `test_psp_network_error_invoice_returns_to_open`      | PSP returns 500; invoice rolls back to `Open`                        |

---

## Design Documents

| File                                | Contents                                                                        |
| ----------------------------------- | ------------------------------------------------------------------------------- |
| [`DESIGN.md`](./docs/DESIGN.md)     | Data model, state machine, payment correctness, webhook design, production gaps |
| [`AI_USAGE.md`](./docs/AI_USAGE.md) | AI tools used, independent decisions, corrections made                          |
| [`API_DOCS.md`](./docs/API_DOCS.md) | Full API reference with request/response examples                               |

---

## Demo Video

A complete walkthrough of the application, including successful and failed payment flows, is available in the demo video below:

https://drive.google.com/file/d/1VIei_KYGgJxJuSmECK52iRtkewjbrApf/view?usp=sharing

## Postman Collection

The Postman collection used during the demo has also been included in this repository. It contains all the API requests required to reproduce and verify the demonstrated functionality.