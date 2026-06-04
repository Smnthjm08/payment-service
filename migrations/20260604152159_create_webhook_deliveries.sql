-- Add migration script here
CREATE TABLE
    webhook_deliveries (
        id UUID PRIMARY KEY,
        endpoint_id UUID NOT NULL REFERENCES webhook_endpoints (id) ON DELETE CASCADE,
        invoice_id UUID REFERENCES invoices (id),
        event_type TEXT NOT NULL,
        payload JSONB NOT NULL,
        status TEXT NOT NULL,
        attempt_count INTEGER NOT NULL DEFAULT 0,
        next_retry_at TIMESTAMPTZ,
        last_error TEXT,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW ()
    );

CREATE INDEX idx_webhook_deliveries_endpoint_id ON webhook_deliveries (endpoint_id);

CREATE INDEX idx_webhook_deliveries_status ON webhook_deliveries (status);

CREATE INDEX idx_webhook_deliveries_invoice_id ON webhook_deliveries (invoice_id);