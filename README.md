# Payment Service

A mock billing API implementing API-key auth, customer/invoice management, an invoice state
machine, idempotent PSP-backed payments, signed async webhook delivery, and the integration
tests described below.

---

## Quick start (Docker — recommended)

```bash
docker compose up --build
```

That single command:
1. Starts **Postgres** and waits until it is healthy.
2. Runs all **sqlx migrations** via the `migrate` container.
3. Starts the **mock-psp** on port `9090` and waits until it is healthy.
4. Starts the **billing** API on port `8080`, pre-wired to Postgres and the mock-psp.

Services are available at:
- Billing API: `http://localhost:8080`
- Mock PSP: `http://localhost:9090`
- Postgres: `localhost:5432`

To stop everything:

```bash
docker compose down          # keep the DB volume
docker compose down -v       # wipe the DB too
```

---

## Quick start (local / native)

```bash
# copy and fill in secrets
cp .env.example .env

# apply migrations
sqlx migrate run

# start the mock PSP (separate terminal)
cargo run --bin mock-psp

# start the billing API
cargo run --bin billing
```

The billing API binds to `0.0.0.0:8080` by default (override with `PORT=`).  
The mock PSP binds to `0.0.0.0:9090` by default (override with `MOCK_PSP_PORT=`).

---

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `DATABASE_URL` | — | PostgreSQL connection string (required) |
| `PSP_URL` | `http://localhost:9090` | Base URL of the mock PSP |
| `PORT` | `8080` | Billing API listen port |
| `MOCK_PSP_PORT` | `9090` | Mock PSP listen port |
| `RUST_LOG` | — | Log level e.g. `billing=debug,info` |

---

## Migrations

```bash
sqlx migrate add '<name>'   # create a new migration file
sqlx migrate run            # apply pending migrations
sqlx database reset         # drop + recreate + re-migrate (dev only)
```

---

## Integration tests

The spec requires three focused integration tests. They live in
[`billing/tests/payment_integration_tests.rs`](billing/tests/payment_integration_tests.rs).

### What each test does

| # | Test name | What it verifies |
|---|---|---|
| 1 | `test_concurrent_pay_only_one_succeeds` | N=10 concurrent `POST /pay` for the **same invoice** — exactly one succeeds (200), the rest get 422; final state is `Paid`; exactly one `Succeeded` attempt in the DB. |
| 2 | `test_idempotent_pay_replays_without_second_psp_call` | Replaying the same `Idempotency-Key` returns an identical response body; the PSP is called **exactly once** across both requests; only one `payment_attempts` row exists. |
| 3 | `test_psp_timeout_invoice_not_stuck_in_processing` | When the PSP sleeps 35 s and billing's client times out at 10 s, the invoice rolls back from `Processing` → `Open` (not stuck); the attempt is marked `TimedOut`. |
| 3b | `test_psp_network_error_invoice_returns_to_open` | PSP returns HTTP 500 → invoice rolls back to `Open`; attempt marked `Error`. |

### How they work

Each test:
1. **Gets an isolated Postgres schema** via `#[sqlx::test(migrations = "../migrations")]` — no shared state between tests.
2. **Starts an in-process mock PSP** on an ephemeral port (`127.0.0.1:0`) using actix-web — no external process needed.
3. **Starts a real billing HTTP server** on another ephemeral port using the same `configure_routes` factory as production.
4. **Fires real HTTP requests** via `reqwest` — including `join_all` for the concurrency test — so the assertions cover the full stack including middleware, handlers, and the DB state machine.

### Running the tests

```bash
# DATABASE_URL must point to a running Postgres instance.
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres

# Run only the integration tests (each test ~100 ms; timeout test ~10 s):
cargo test --test payment_integration_tests -- --nocapture

# Or run everything:
cargo test
```

> **Note on test 3 (timeout):** This test takes ~10 seconds intentionally — it lets
> billing's 10-second PSP client timeout fire so we can assert the invoice is not
> left in `Processing`. This is expected and correct behaviour.

### What is NOT tested (and why)

We do not write a test per handler (create customer, get invoice, etc.).
Those handlers are thin wrappers around typed sqlx queries; the behaviour
worth testing is the **payment lifecycle**, which is what tests 1–3 cover.
The spec explicitly endorses this approach:

> *"Lean on these rather than testing every handler."*

---

## Design notes

See [`docs/DESIGN.md`](docs/DESIGN.md) for:

- API key storage, hashing, and revocation rationale
- Invoice state machine diagram (Mermaid)
- Two-phase commit pattern for PSP calls
- Webhook signing and retry backoff schedule
- Mock PSP token table
