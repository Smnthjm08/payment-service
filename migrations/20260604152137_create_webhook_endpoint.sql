-- Add migration script here
CREATE TABLE
    webhook_endpoints (
        id UUID PRIMARY KEY,
        business_id UUID NOT NULL REFERENCES businesses (id) ON DELETE CASCADE,
        url TEXT NOT NULL,
        secret TEXT NOT NULL,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW ()
    );

CREATE INDEX idx_webhook_endpoints_business_id ON webhook_endpoints (business_id);