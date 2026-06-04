-- Add migration script here
CREATE TABLE
    invoices (
        id UUID PRIMARY KEY,
        business_id UUID NOT NULL REFERENCES businesses (id) ON DELETE CASCADE,
        customer_id UUID NOT NULL REFERENCES customers (id) ON DELETE CASCADE,
        state TEXT NOT NULL,
        total_amount_cents BIGINT NOT NULL,
        due_date TIMESTAMPTZ NOT NULL,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
        paid_at TIMESTAMPTZ
    );

CREATE INDEX idx_invoices_business_id ON invoices (business_id);

CREATE INDEX idx_invoices_customer_id ON invoices (customer_id);

CREATE INDEX idx_invoices_state ON invoices (state);