-- Add migration script here
CREATE TABLE
    payment_attempts (
        id UUID PRIMARY KEY,
        invoice_id UUID NOT NULL REFERENCES invoices (id) ON DELETE CASCADE,
        status TEXT NOT NULL,
        amount_cents BIGINT NOT NULL,
        card_token TEXT NOT NULL,
        psp_ref TEXT,
        psp_response JSONB,
        failure_code TEXT,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW ()
    );

CREATE INDEX idx_payment_attempts_invoice_id ON payment_attempts (invoice_id);
