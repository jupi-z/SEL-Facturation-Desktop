CREATE TABLE organizations (
    id UUID PRIMARY KEY,
    legal_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id),
    role TEXT NOT NULL CHECK (role IN ('ADMIN','BILLING_AGENT','CASHIER','SUPERVISOR','AUDITOR_READONLY')),
    status TEXT NOT NULL CHECK (status IN ('ACTIVE','REVOKED')),
    identity_ref TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE devices (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id),
    public_key_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('ACTIVE','REVOKED')),
    last_seen TIMESTAMPTZ,
    sync_cursor BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE offline_grants (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    device_id UUID NOT NULL REFERENCES devices(id),
    scopes JSONB NOT NULL,
    limits JSONB NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    signature TEXT NOT NULL
);

CREATE TABLE customers (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id),
    customer_code TEXT NOT NULL,
    legal_name TEXT NOT NULL,
    contact_fields JSONB NOT NULL DEFAULT '{}'::jsonb,
    status TEXT NOT NULL CHECK (status IN ('ACTIVE','ARCHIVED')),
    revision BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, customer_code)
);

CREATE TABLE service_accounts (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL REFERENCES customers(id),
    service_code TEXT NOT NULL CHECK (service_code IN ('ELECTRICITY','WATER')),
    contract_ref TEXT,
    connection_status TEXT NOT NULL CHECK (connection_status IN ('CONNECTED','NOT_CONNECTED','UNKNOWN')),
    activated_at DATE
);

CREATE TABLE meters (
    id UUID PRIMARY KEY,
    service_account_id UUID NOT NULL REFERENCES service_accounts(id),
    serial TEXT NOT NULL,
    unit TEXT NOT NULL CHECK (unit IN ('KWH','M3')),
    active BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE meter_readings (
    id UUID PRIMARY KEY,
    meter_id UUID NOT NULL REFERENCES meters(id),
    period TEXT NOT NULL,
    previous NUMERIC(20,6) NOT NULL CHECK (previous >= 0),
    current NUMERIC(20,6) NOT NULL CHECK (current >= previous),
    measured_quantity NUMERIC(20,6) NOT NULL CHECK (measured_quantity >= 0),
    source TEXT NOT NULL,
    corrected_from_id UUID REFERENCES meter_readings(id)
);

CREATE TABLE products (
    id UUID PRIMARY KEY,
    sku TEXT NOT NULL UNIQUE,
    product_type TEXT NOT NULL CHECK (product_type IN ('SOLAR_KIT','OTHER')),
    label TEXT NOT NULL,
    price_reference NUMERIC(20,6) NOT NULL CHECK (price_reference >= 0),
    currency TEXT NOT NULL,
    active BOOLEAN NOT NULL
);

CREATE TABLE tariff_versions (
    id UUID PRIMARY KEY,
    service_code TEXT NOT NULL CHECK (service_code IN ('ELECTRICITY','WATER')),
    customer_category TEXT NOT NULL,
    unit TEXT NOT NULL CHECK (unit IN ('KWH','M3')),
    unit_price NUMERIC(20,6) NOT NULL CHECK (unit_price >= 0),
    currency TEXT NOT NULL,
    effective_from DATE NOT NULL,
    effective_to DATE,
    approval_ref TEXT NOT NULL,
    approved_by UUID REFERENCES users(id),
    status TEXT NOT NULL CHECK (status IN ('DRAFT','APPROVED','REVOKED'))
);

CREATE TABLE tax_profile_versions (
    id UUID PRIMARY KEY,
    service_code TEXT NOT NULL CHECK (service_code IN ('ELECTRICITY','WATER')),
    jurisdiction TEXT NOT NULL,
    base TEXT NOT NULL,
    rate NUMERIC(20,6) NOT NULL CHECK (rate >= 0),
    source TEXT NOT NULL,
    effective_from DATE NOT NULL,
    effective_to DATE,
    status TEXT NOT NULL CHECK (status IN ('DRAFT','APPROVED','REVOKED'))
);

CREATE TABLE fx_rate_versions (
    id UUID PRIMARY KEY,
    from_currency TEXT NOT NULL,
    to_currency TEXT NOT NULL,
    value NUMERIC(20,12) NOT NULL CHECK (value > 0),
    effective_from DATE NOT NULL,
    source TEXT NOT NULL,
    approved BOOLEAN NOT NULL
);

CREATE TABLE contracts (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL REFERENCES customers(id),
    contract_type TEXT NOT NULL CHECK (contract_type IN ('UTILITY','SOLAR_INSTALLMENT')),
    status TEXT NOT NULL CHECK (status IN ('DRAFT','VALIDATED','ACTIVE','CLOSED','CANCELLED')),
    terms_revision TEXT NOT NULL,
    signed_at DATE
);

CREATE TABLE sales (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL REFERENCES customers(id),
    contract_id UUID REFERENCES contracts(id),
    currency TEXT NOT NULL,
    sold_at TIMESTAMPTZ NOT NULL,
    sale_type TEXT NOT NULL CHECK (sale_type IN ('SOLAR_KIT','OTHER')),
    status TEXT NOT NULL
);

CREATE TABLE sale_lines (
    id UUID PRIMARY KEY,
    sale_id UUID NOT NULL REFERENCES sales(id),
    product_id UUID NOT NULL REFERENCES products(id),
    quantity NUMERIC(20,6) NOT NULL CHECK (quantity > 0),
    unit_price NUMERIC(20,6) NOT NULL CHECK (unit_price >= 0),
    tax_snapshot JSONB NOT NULL,
    total NUMERIC(20,6) NOT NULL CHECK (total >= 0)
);

CREATE TABLE invoices (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL REFERENCES customers(id),
    sale_id UUID REFERENCES sales(id),
    period TEXT,
    invoice_number TEXT,
    provisional_ref TEXT,
    source_device_id UUID REFERENCES devices(id),
    issued_at TIMESTAMPTZ NOT NULL,
    currency TEXT NOT NULL,
    total NUMERIC(20,6) NOT NULL CHECK (total >= 0),
    template_version TEXT NOT NULL,
    immutable_snapshot JSONB NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('LOCAL_PENDING','CENTRAL_ACCEPTED','VOID','CONFLICT_REQUIRES_REVIEW')),
    version BIGINT NOT NULL DEFAULT 1
);

CREATE TABLE invoice_lines (
    id UUID PRIMARY KEY,
    invoice_id UUID NOT NULL REFERENCES invoices(id),
    service_code TEXT CHECK (service_code IN ('ELECTRICITY','WATER')),
    product_ref TEXT,
    quantity NUMERIC(20,6) NOT NULL CHECK (quantity >= 0),
    unit TEXT NOT NULL,
    unit_price NUMERIC(20,6),
    subtotal NUMERIC(20,6) NOT NULL CHECK (subtotal >= 0),
    tax_snapshot JSONB NOT NULL,
    total NUMERIC(20,6) NOT NULL CHECK (total >= 0)
);

CREATE TABLE receivables (
    id UUID PRIMARY KEY,
    source_type TEXT NOT NULL CHECK (source_type IN ('INVOICE','SALE')),
    source_id UUID NOT NULL,
    customer_id UUID NOT NULL REFERENCES customers(id),
    currency TEXT NOT NULL,
    principal_total NUMERIC(20,6) NOT NULL CHECK (principal_total >= 0),
    status TEXT NOT NULL
);

CREATE TABLE installment_plans (
    id UUID PRIMARY KEY,
    receivable_id UUID NOT NULL UNIQUE REFERENCES receivables(id),
    contract_id UUID NOT NULL REFERENCES contracts(id),
    principal_amount NUMERIC(20,6) NOT NULL CHECK (principal_amount >= 0),
    currency TEXT NOT NULL,
    start_date DATE NOT NULL,
    status TEXT NOT NULL,
    terms_snapshot JSONB NOT NULL
);

CREATE TABLE installment_schedule (
    id UUID PRIMARY KEY,
    plan_id UUID NOT NULL REFERENCES installment_plans(id),
    sequence INTEGER NOT NULL,
    due_date DATE NOT NULL,
    amount_due NUMERIC(20,6) NOT NULL CHECK (amount_due >= 0),
    schedule_status TEXT NOT NULL,
    UNIQUE (plan_id, sequence)
);

CREATE TABLE payments (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL REFERENCES customers(id),
    amount_received NUMERIC(20,6) NOT NULL CHECK (amount_received > 0),
    currency TEXT NOT NULL,
    method TEXT NOT NULL,
    paid_at TIMESTAMPTZ NOT NULL,
    external_ref TEXT,
    origin_event_id UUID UNIQUE,
    status TEXT NOT NULL
);

CREATE TABLE payment_allocations (
    id UUID PRIMARY KEY,
    payment_id UUID NOT NULL REFERENCES payments(id),
    receivable_id UUID NOT NULL REFERENCES receivables(id),
    schedule_id UUID REFERENCES installment_schedule(id),
    allocated_amount NUMERIC(20,6) NOT NULL CHECK (allocated_amount > 0),
    currency TEXT NOT NULL
);

CREATE TABLE payment_reversals (
    id UUID PRIMARY KEY,
    payment_id UUID NOT NULL REFERENCES payments(id),
    amount NUMERIC(20,6) NOT NULL CHECK (amount > 0),
    reason TEXT NOT NULL,
    initiated_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE audit_events (
    id UUID PRIMARY KEY,
    actor UUID,
    device UUID,
    event_type TEXT NOT NULL,
    subject_id UUID,
    occurred_at TIMESTAMPTZ NOT NULL,
    redacted_metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE idempotency_keys (
    organization_id UUID NOT NULL REFERENCES organizations(id),
    principal_id UUID NOT NULL,
    operation TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    response_status INTEGER NOT NULL,
    response_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, principal_id, operation, idempotency_key)
);

CREATE TABLE sync_inbox (
    device_id UUID NOT NULL REFERENCES devices(id),
    event_id UUID NOT NULL,
    payload_sha256 TEXT NOT NULL,
    result_json JSONB NOT NULL,
    accepted_sequence BIGINT,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (device_id, event_id)
);

CREATE TABLE central_changes (
    sequence BIGSERIAL PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id),
    entity TEXT NOT NULL,
    entity_id UUID NOT NULL,
    revision BIGINT NOT NULL,
    change_json JSONB NOT NULL
);

CREATE TABLE sync_conflicts (
    id UUID PRIMARY KEY,
    device_id UUID NOT NULL REFERENCES devices(id),
    event_id UUID NOT NULL,
    reason TEXT NOT NULL,
    central_revision BIGINT,
    payload_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE document_template_registry (
    template_id TEXT PRIMARY KEY,
    semantic_version TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    min_client_version TEXT NOT NULL,
    activated_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX one_utility_invoice_period ON invoices(customer_id, period)
    WHERE period IS NOT NULL AND status <> 'VOID';
CREATE INDEX invoices_customer_idx ON invoices(customer_id, issued_at DESC);
CREATE INDEX changes_org_sequence_idx ON central_changes(organization_id, sequence);

