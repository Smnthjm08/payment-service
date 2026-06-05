# AI Usage & Independent Decisions

This document details how AI was leveraged during the development of this Payment Service, and more importantly, where human architectural decisions overrode AI suggestions to satisfy the correctness, reliability, and multi-tenancy requirements of the assignment.

---

## 1. AI Tools Used and Their Purpose

### Cursor / Autocomplete

Used heavily for:

- Writing Actix-web handler boilerplate.
- Generating SQLx CRUD repository methods.
- Generating request/response DTOs.
- Assisting with enum serialization and deserialization.

Examples include customer, invoice, and API key handlers where the implementation pattern was repetitive and low risk.

### ChatGPT / Claude

Used primarily for:

- Schema brainstorming.
- Evaluating PostgreSQL concurrency patterns.
- Reviewing repository design.
- Exploring idempotency strategies.
- Discussing webhook delivery architecture.
- Reviewing transaction safety.

One example was evaluating PostgreSQL advisory locks for preventing duplicate payments. After reviewing the tradeoffs, I chose not to use advisory locks because they require holding database resources during external PSP calls. Instead, I implemented optimistic concurrency through invoice state transitions.

### GitHub Copilot

Used to accelerate:

- Tokio task spawning patterns.
- Actix middleware scaffolding.
- Error propagation boilerplate.
- Struct generation and repetitive Rust code.

AI was treated as an implementation assistant rather than a source of architectural truth.

---

## 2. Independent Architectural Decisions

### 1. Customer Email Uniqueness

**What the AI proposed:**

A global constraint:

```sql
UNIQUE(email)
```

**What I chose:**

```sql
UNIQUE(business_id, email)
```

**Why:**

This system is multi-tenant. A single customer email address may legitimately appear under multiple businesses using the platform.

Global uniqueness would incorrectly prevent valid customer creation.

---

### 2. Idempotency Storage

**What the AI proposed:**

Adding an `idempotency_key` column directly to the `payment_attempts` table.

**What I chose:**

A dedicated `idempotency_keys` table storing:

- key
- request_hash
- response_body
- status_code
- expires_at

**Why:**

True idempotency requires replaying the exact previous response without calling the PSP again.

Separating idempotency concerns into a dedicated table also allows request hash validation and response caching.

---

### 3. Webhook Traceability

**What the AI proposed:**

A generic webhook delivery table containing only:

- endpoint_id
- event_type
- payload

**What I chose:**

I added:

```sql
invoice_id UUID
```

with an index.

**Why:**

Support and debugging become significantly easier when engineers can immediately answer:

> "What happened to the webhook for Invoice X?"

without searching through JSON payloads.

---

### 4. Invoice Total Calculation

**What the AI proposed:**

Accepting a client-provided `total_amount_cents` value during invoice creation.

**What I chose:**

The API only accepts line items:

- description
- quantity
- unit_amount_cents

The server calculates:

```text
quantity × unit_amount_cents
```

for each line item and computes the final invoice total.

**Why:**

Trusting client-supplied totals introduces integrity issues and allows mismatches between invoice totals and line items.

Server-side calculation guarantees correctness and satisfies the assignment requirement.

---

### 5. Payment Concurrency Control

**What the AI proposed:**

Application-level synchronization using mutexes.

**What I chose:**

Database-enforced optimistic locking through invoice state transitions:

```sql
UPDATE invoices
SET state = 'Processing'
WHERE id = $1
AND state = 'Open'
```

**Why:**

Application-level locks only work within a single process.

Database-level transitions continue to work correctly when the service is horizontally scaled across multiple instances.

This guarantees only one payment flow can proceed for a given invoice.

---

### 6. API Key Storage Strategy

**What the AI proposed:**

Storing raw API keys directly in the database.

**What I chose:**

Storing:

- key_prefix (plaintext)
- key_hash (SHA-256)

while never persisting the plaintext API key.

**Why:**

API keys are shown only once during creation.

If the database is compromised, attackers cannot directly use stored credentials.

The prefix enables efficient lookup while the hash enables verification without storing secrets.

---

## 3. What the AI Got Wrong and Required Manual Correction

### 1. Missing Database Transactions

During early iterations, AI-generated invoice creation logic inserted:

1. Invoice
2. Line Item 1
3. Line Item 2
4. Line Item N

using independent database calls.

**Problem:**

A failure midway through insertion could leave a partially-created invoice in the database.

**Correction:**

I wrapped the operation inside a PostgreSQL transaction:

```rust
let mut tx = pool.begin().await?;
```

and executed all queries through the transaction before committing. The same correction was applied to the Business + API Key creation flow.

---

### 2. Missing Multi-Tenant Authorization Checks

Early repository implementations performed lookups using only primary keys.

Example:

```sql
SELECT * FROM customers
WHERE id = $1
```

**Problem:**

A business could potentially access another business's records if a UUID became known.

**Correction:**

I added tenant scoping to every query:

```sql
SELECT * FROM customers
WHERE id = $1
AND business_id = $2
```

The same pattern is applied to invoices, webhooks, and all other tenant-owned resources.

---

### 3. Missing State Validation

Early AI-generated payment logic focused on calling the PSP directly without validating the current invoice state.

**Problem:**

Invoices in terminal states (`Paid`, `Void`, `Uncollectible`) could potentially be charged again. Invoices in `Draft` state could also be paid without being finalized.

**Correction:**

I introduced an explicit invoice state machine with enforced valid and invalid transitions. The atomic `UPDATE ... WHERE state = 'Open'` ensures:

- Only `Open` invoices can be charged.
- `Draft` invoices must be finalized first.
- Terminal states reject all further payment attempts with `422 Unprocessable Entity`.

This prevents invalid business operations and duplicate charging scenarios.

---

## 4. Summary

AI significantly accelerated implementation by generating boilerplate code, repository scaffolding, Actix handlers, and repetitive SQLx mappings.

However, correctness-critical decisions involving:

- Multi-tenancy
- Payment concurrency
- Idempotency
- Transaction boundaries
- State transitions
- Security
- Failure handling

were reviewed and adjusted manually.

The final architecture reflects deliberate engineering decisions rather than direct acceptance of AI-generated output. AI served as an implementation assistant, while architectural ownership remained with the developer.
