-- Add migration script here
CREATE TABLE
    idempotency_keys (
        id UUID PRIMARY KEY,
        business_id UUID NOT NULL REFERENCES businesses (id) ON DELETE CASCADE,
        key TEXT NOT NULL,
        request_hash TEXT NOT NULL,
        response_body JSONB,
        status_code INTEGER,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
        expires_at TIMESTAMPTZ NOT NULL DEFAULT (NOW () + INTERVAL '24 hours'),
        UNIQUE (business_id, key)
    );

CREATE INDEX idx_idempotency_keys_business_id ON idempotency_keys (business_id);