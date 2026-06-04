-- Add migration script here
CREATE TABLE
    customers (
        id UUID PRIMARY KEY,
        business_id UUID NOT NULL REFERENCES businesses (id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        email TEXT NOT NULL,
        phone TEXT,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
        UNIQUE (business_id, email)
    );

CREATE INDEX idx_customers_business_id ON customers (business_id);