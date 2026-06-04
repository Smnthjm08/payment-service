-- Add migration script here
CREATE TABLE
    invoice_line_items (
        id UUID PRIMARY KEY,
        invoice_id UUID NOT NULL REFERENCES invoices (id) ON DELETE CASCADE,
        description TEXT NOT NULL,
        quantity INTEGER NOT NULL,
        unit_amount_cents BIGINT NOT NULL,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW ()
    );

CREATE INDEX idx_invoice_line_items_invoice_id ON invoice_line_items (invoice_id);