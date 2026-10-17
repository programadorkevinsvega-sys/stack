
CREATE TABLE IF NOT EXISTS invoice_sequences (
    period_key CHAR(6) PRIMARY KEY,
    next_value BIGINT NOT NULL DEFAULT 1 CHECK (next_value > 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS invoices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    subscription_id UUID,
    invoice_number VARCHAR(30) NOT NULL UNIQUE,
    period_start TIMESTAMPTZ NOT NULL,
    period_end TIMESTAMPTZ NOT NULL,
    subtotal NUMERIC(14,4) NOT NULL,
    tax NUMERIC(14,4) NOT NULL DEFAULT 0,
    total NUMERIC(14,4) NOT NULL,
    currency CHAR(3) NOT NULL DEFAULT 'USD',
    status VARCHAR(20) NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft','open','paid','void','uncollectible')),
    due_date TIMESTAMPTZ,
    paid_at TIMESTAMPTZ,
    pdf_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_invoice_period CHECK (period_start < period_end),
    CONSTRAINT chk_invoice_total CHECK (total = subtotal + tax)
);

CREATE INDEX IF NOT EXISTS idx_invoices_user_created_at
    ON invoices(user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_invoices_status_due_date
    ON invoices(status, due_date);

CREATE TABLE IF NOT EXISTS invoice_line_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    description VARCHAR(300) NOT NULL,
    quantity NUMERIC(14,4) NOT NULL,
    unit_price NUMERIC(14,6) NOT NULL,
    amount NUMERIC(14,4) NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_invoice_line_items_invoice_id
    ON invoice_line_items(invoice_id);
