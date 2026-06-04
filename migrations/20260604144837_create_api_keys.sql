-- Add migration script here
CREATE TABLE
    api_keys (
        id UUID PRIMARY KEY,
        business_id UUID NOT NULL REFERENCES businesses (id) ON DELETE CASCADE,
        key_prefix TEXT NOT NULL UNIQUE,
        key_hash TEXT NOT NULL,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
        revoked_at TIMESTAMPTZ
    );

CREATE INDEX idx_api_keys_business_id
ON api_keys (business_id);