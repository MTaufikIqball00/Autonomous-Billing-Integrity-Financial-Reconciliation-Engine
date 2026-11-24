-- =============================================================================
-- ABIFR — Initial PostgreSQL Schema
-- Strict separation of raw extracted facts, normalized financial values,
-- deterministic reconciliation calculations, AI runs, and human decisions.
-- =============================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- -----------------------------------------------------------------------------
-- 1. RBAC, Users & Sessions
-- -----------------------------------------------------------------------------
CREATE TABLE roles (
    id SMALLSERIAL PRIMARY KEY,
    name VARCHAR(32) NOT NULL UNIQUE,
    description TEXT NOT NULL,
    permissions JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO roles (name, description, permissions) VALUES
('admin', 'System administrator with full access including settings and user management', '["*"]'::jsonb),
('reviewer', 'Financial reviewer authorized to approve, reject, correct extractions, and resolve findings', '["documents:read","documents:write","reconciliation:run","findings:read","findings:review","reports:read","reports:export","vendors:write","contracts:write","payments:write"]'::jsonb),
('analyst', 'Financial analyst authorized to upload documents, run reconciliations, and view findings', '["documents:read","documents:write","reconciliation:run","findings:read","reports:read","reports:export"]'::jsonb),
('auditor', 'Read-only auditor with full access to evidence, audit logs, and reports', '["documents:read","findings:read","reports:read","reports:export","audit:read"]'::jsonb);

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL UNIQUE,
    full_name VARCHAR(255) NOT NULL,
    password_hash TEXT NOT NULL,
    role_name VARCHAR(32) NOT NULL REFERENCES roles(name),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_email ON users(email);

-- -----------------------------------------------------------------------------
-- 2. Vendors
-- -----------------------------------------------------------------------------
CREATE TABLE vendors (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    vendor_code VARCHAR(64) NOT NULL UNIQUE,
    legal_name VARCHAR(255) NOT NULL,
    tax_id VARCHAR(128),
    default_currency VARCHAR(8),
    country_code VARCHAR(8),
    payment_terms VARCHAR(128),
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_vendors_legal_name ON vendors(LOWER(legal_name));

-- -----------------------------------------------------------------------------
-- 3. Documents & Immutable Storage Provenance
-- -----------------------------------------------------------------------------
CREATE TABLE documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    original_filename VARCHAR(512) NOT NULL,
    stored_blob_key VARCHAR(128) NOT NULL, -- content-addressed SHA-256 key only (never raw fs path)
    sha256_hash CHAR(64) NOT NULL,
    file_size_bytes BIGINT NOT NULL CHECK (file_size_bytes > 0),
    declared_mime_type VARCHAR(128) NOT NULL,
    detected_mime_type VARCHAR(128) NOT NULL,
    file_extension VARCHAR(16) NOT NULL,
    page_count INTEGER,
    -- Classification separates AI prediction vs effective (human-overridable) category
    ai_category VARCHAR(64) NOT NULL DEFAULT 'unknown',
    ai_classification_confidence NUMERIC(6,4), -- NULL if not supplied
    effective_category VARCHAR(64) NOT NULL DEFAULT 'unknown',
    classification_locked_by UUID REFERENCES users(id),
    classification_locked_at TIMESTAMPTZ,
    -- Processing lifecycle
    processing_status VARCHAR(32) NOT NULL DEFAULT 'pending', -- pending|processing|completed|failed|requires_review
    ocr_status VARCHAR(32) NOT NULL DEFAULT 'pending',
    extraction_status VARCHAR(32) NOT NULL DEFAULT 'pending',
    matching_status VARCHAR(32) NOT NULL DEFAULT 'pending',
    reconciliation_status VARCHAR(32) NOT NULL DEFAULT 'pending',
    failure_reason TEXT,
    is_duplicate_file BOOLEAN NOT NULL DEFAULT FALSE,
    duplicate_of_document_id UUID REFERENCES documents(id),
    prompt_injection_flagged BOOLEAN NOT NULL DEFAULT FALSE,
    vendor_id UUID REFERENCES vendors(id),
    uploaded_by UUID REFERENCES users(id),
    is_synthetic_fixture BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_documents_sha256 ON documents(sha256_hash);
CREATE INDEX idx_documents_status ON documents(processing_status);
CREATE INDEX idx_documents_category ON documents(effective_category);
CREATE INDEX idx_documents_vendor ON documents(vendor_id);

CREATE TABLE document_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    version_number INTEGER NOT NULL,
    sha256_hash CHAR(64) NOT NULL,
    stored_blob_key VARCHAR(128) NOT NULL,
    change_note TEXT,
    created_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(document_id, version_number)
);

-- -----------------------------------------------------------------------------
-- 4. Raw Extractions & Evidence Provenance (IMMUTABLE)
-- -----------------------------------------------------------------------------
CREATE TABLE document_extractions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    extraction_method VARCHAR(64) NOT NULL, -- pymupdf_text|rapidocr_ppocrv4|docx_parser|xlsx_parser|csv_parser|openai_vlm|gemini_vlm
    provider_name VARCHAR(64) NOT NULL,
    model_version VARCHAR(128) NOT NULL,
    raw_text TEXT NOT NULL,
    pages_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    tables_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    structured_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    prompt_injection_signals JSONB NOT NULL DEFAULT '[]'::jsonb,
    redaction_applied BOOLEAN NOT NULL DEFAULT FALSE,
    duration_ms INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_extractions_doc ON document_extractions(document_id);

CREATE TABLE extracted_fields (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    extraction_id UUID NOT NULL REFERENCES document_extractions(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    field_name VARCHAR(128) NOT NULL,
    line_item_index INTEGER, -- NULL for header fields
    extracted_value TEXT, -- NULL if model/parser could not identify
    original_text TEXT,
    page_number INTEGER,
    bounding_box JSONB, -- {"x0":..., "y0":..., "x1":..., "y1":..., "page_width":..., "page_height":...} or NULL
    source_location TEXT, -- e.g. "page:1:line:4" or "sheet:Invoice!C12"
    extraction_method VARCHAR(64) NOT NULL,
    provider_confidence NUMERIC(6,4), -- NEVER fabricated; NULL when unavailable
    validation_status VARCHAR(32) NOT NULL DEFAULT 'unverified', -- unverified|valid|ambiguous|missing|human_corrected
    -- Normalized deterministic output (populated by Rust normalizer; raw value stays untouched)
    normalized_value TEXT,
    normalized_decimal NUMERIC(20,6),
    normalized_currency VARCHAR(8),
    normalization_rule VARCHAR(128),
    -- Effective human override (preserves original extracted_value)
    human_override_value TEXT,
    human_override_by UUID REFERENCES users(id),
    human_override_at TIMESTAMPTZ,
    human_override_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_extracted_fields_doc ON extracted_fields(document_id);
CREATE INDEX idx_extracted_fields_name ON extracted_fields(document_id, field_name);

-- -----------------------------------------------------------------------------
-- 5. Contracts, Versions, Amendments & Line Items
-- -----------------------------------------------------------------------------
CREATE TABLE contracts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    contract_number VARCHAR(128) NOT NULL UNIQUE,
    vendor_id UUID NOT NULL REFERENCES vendors(id),
    title VARCHAR(255) NOT NULL,
    currency VARCHAR(8) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'active', -- draft|active|expired|terminated
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE contract_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    contract_id UUID NOT NULL REFERENCES contracts(id) ON DELETE CASCADE,
    document_id UUID REFERENCES documents(id),
    version_label VARCHAR(64) NOT NULL,
    version_number INTEGER NOT NULL,
    is_amendment BOOLEAN NOT NULL DEFAULT FALSE,
    amends_version_id UUID REFERENCES contract_versions(id),
    effective_date DATE NOT NULL,
    expiration_date DATE NOT NULL,
    payment_terms VARCHAR(128),
    -- Contract-specific tolerances: NULL means tolerance policy is UNDEFINED (never invented!)
    price_tolerance_pct NUMERIC(10,6),
    price_tolerance_abs NUMERIC(20,6),
    quantity_tolerance_pct NUMERIC(10,6),
    default_tax_rate NUMERIC(10,6),
    tax_provision_notes TEXT,
    allowed_additional_charges JSONB NOT NULL DEFAULT '[]'::jsonb,
    price_adjustment_clause TEXT,
    exceptions_clause TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(contract_id, version_number),
    CHECK (expiration_date >= effective_date)
);

CREATE INDEX idx_contract_versions_dates ON contract_versions(contract_id, effective_date, expiration_date);

CREATE TABLE contract_line_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    contract_version_id UUID NOT NULL REFERENCES contract_versions(id) ON DELETE CASCADE,
    item_code VARCHAR(128),
    description TEXT NOT NULL,
    unit VARCHAR(64) NOT NULL,
    unit_price NUMERIC(20,6) NOT NULL,
    currency VARCHAR(8) NOT NULL,
    min_quantity NUMERIC(20,6),
    max_quantity NUMERIC(20,6),
    discount_pct NUMERIC(10,6) DEFAULT 0,
    tax_rate NUMERIC(10,6),
    source_field_id UUID REFERENCES extracted_fields(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_contract_items_version ON contract_line_items(contract_version_id);

-- -----------------------------------------------------------------------------
-- 6. Purchase Orders
-- -----------------------------------------------------------------------------
CREATE TABLE purchase_orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    po_number VARCHAR(128) NOT NULL UNIQUE,
    vendor_id UUID NOT NULL REFERENCES vendors(id),
    contract_id UUID REFERENCES contracts(id),
    document_id UUID REFERENCES documents(id),
    issue_date DATE NOT NULL,
    currency VARCHAR(8) NOT NULL,
    total_amount NUMERIC(20,6) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'issued',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE purchase_order_line_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    purchase_order_id UUID NOT NULL REFERENCES purchase_orders(id) ON DELETE CASCADE,
    item_code VARCHAR(128),
    description TEXT NOT NULL,
    unit VARCHAR(64) NOT NULL,
    ordered_quantity NUMERIC(20,6) NOT NULL,
    received_quantity NUMERIC(20,6),
    unit_price NUMERIC(20,6) NOT NULL,
    line_total NUMERIC(20,6) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- -----------------------------------------------------------------------------
-- 7. Invoices & Invoice Line Items
-- -----------------------------------------------------------------------------
CREATE TABLE invoices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID REFERENCES documents(id),
    invoice_number VARCHAR(128) NOT NULL,
    vendor_id UUID NOT NULL REFERENCES vendors(id),
    customer_name VARCHAR(255),
    invoice_date DATE NOT NULL,
    due_date DATE,
    currency VARCHAR(8) NOT NULL,
    referenced_contract_number VARCHAR(128),
    referenced_po_number VARCHAR(128),
    matched_contract_id UUID REFERENCES contracts(id),
    matched_contract_version_id UUID REFERENCES contract_versions(id),
    matched_po_id UUID REFERENCES purchase_orders(id),
    match_confidence_status VARCHAR(32) NOT NULL DEFAULT 'unmatched', -- unmatched|deterministic_matched|ambiguous_needs_review|human_confirmed
    -- Stated (as extracted from document) vs Deterministic Recalculated
    stated_subtotal NUMERIC(20,6) NOT NULL,
    stated_discount_total NUMERIC(20,6) NOT NULL DEFAULT 0,
    stated_tax_total NUMERIC(20,6) NOT NULL DEFAULT 0,
    stated_additional_charges NUMERIC(20,6) NOT NULL DEFAULT 0,
    stated_total NUMERIC(20,6) NOT NULL,
    recalculated_subtotal NUMERIC(20,6),
    recalculated_tax_total NUMERIC(20,6),
    recalculated_total NUMERIC(20,6),
    payment_terms VARCHAR(128),
    is_credit_note BOOLEAN NOT NULL DEFAULT FALSE,
    reconciliation_status VARCHAR(32) NOT NULL DEFAULT 'pending', -- pending|processing|reconciled|discrepancy_confirmed|requires_review
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_invoices_vendor_num ON invoices(vendor_id, invoice_number);
CREATE INDEX idx_invoices_status ON invoices(reconciliation_status);
CREATE INDEX idx_invoices_doc ON invoices(document_id);

CREATE TABLE invoice_line_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    line_number INTEGER NOT NULL,
    item_code VARCHAR(128),
    description TEXT NOT NULL,
    quantity NUMERIC(20,6) NOT NULL,
    unit VARCHAR(64) NOT NULL,
    unit_price NUMERIC(20,6) NOT NULL,
    discount_pct NUMERIC(10,6) NOT NULL DEFAULT 0,
    discount_amount NUMERIC(20,6) NOT NULL DEFAULT 0,
    tax_rate NUMERIC(10,6),
    tax_amount NUMERIC(20,6) NOT NULL DEFAULT 0,
    stated_line_subtotal NUMERIC(20,6) NOT NULL,
    stated_line_total NUMERIC(20,6) NOT NULL,
    is_additional_charge BOOLEAN NOT NULL DEFAULT FALSE,
    matched_contract_item_id UUID REFERENCES contract_line_items(id),
    matched_po_item_id UUID REFERENCES purchase_order_line_items(id),
    match_method VARCHAR(64) NOT NULL DEFAULT 'unmatched', -- exact_code|exact_description|fuzzy_candidate|human_confirmed|unmatched
    match_requires_review BOOLEAN NOT NULL DEFAULT FALSE,
    source_page INTEGER,
    source_bbox JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_invoice_items_invoice ON invoice_line_items(invoice_id);

-- -----------------------------------------------------------------------------
-- 8. Payments & Many-to-Many Allocations
-- -----------------------------------------------------------------------------
CREATE TABLE payments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID REFERENCES documents(id),
    payment_reference VARCHAR(128) NOT NULL UNIQUE,
    bank_reference VARCHAR(128),
    vendor_id UUID NOT NULL REFERENCES vendors(id),
    beneficiary_name VARCHAR(255) NOT NULL,
    payment_date DATE NOT NULL,
    amount NUMERIC(20,6) NOT NULL,
    currency VARCHAR(8) NOT NULL,
    referenced_invoice_number VARCHAR(128),
    transaction_status VARCHAR(32) NOT NULL DEFAULT 'settled', -- pending|settled|reversed|failed
    is_refund_or_credit BOOLEAN NOT NULL DEFAULT FALSE,
    allocation_status VARCHAR(32) NOT NULL DEFAULT 'unmatched', -- unmatched|partially_allocated|fully_allocated|over_allocated|requires_review
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_payments_vendor ON payments(vendor_id);
CREATE INDEX idx_payments_ref_inv ON payments(referenced_invoice_number);

CREATE TABLE payment_allocations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id UUID NOT NULL REFERENCES payments(id) ON DELETE CASCADE,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    allocated_amount NUMERIC(20,6) NOT NULL CHECK (allocated_amount > 0),
    currency VARCHAR(8) NOT NULL,
    matching_evidence TEXT NOT NULL, -- explicit reason: invoice_ref+vendor+currency, human_confirmed, etc.
    matched_by VARCHAR(64) NOT NULL DEFAULT 'deterministic_engine',
    confirmed_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(payment_id, invoice_id)
);

-- -----------------------------------------------------------------------------
-- 9. Durable Processing Queue (PostgreSQL SKIP LOCKED + optional Redis signal)
-- -----------------------------------------------------------------------------
CREATE TABLE processing_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    idempotency_key VARCHAR(255) NOT NULL UNIQUE,
    job_type VARCHAR(64) NOT NULL, -- document_pipeline|reconcile_invoice|generate_report
    target_id UUID NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'queued', -- queued|running|completed|failed|dead_letter
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_error TEXT,
    locked_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_processing_jobs_status ON processing_jobs(status, created_at);

-- -----------------------------------------------------------------------------
-- 10. Reconciliation Jobs, Results, Anomaly Findings & Evidence References
-- -----------------------------------------------------------------------------
CREATE TABLE reconciliation_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    idempotency_key VARCHAR(255) NOT NULL UNIQUE,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    contract_version_id UUID REFERENCES contract_versions(id),
    purchase_order_id UUID REFERENCES purchase_orders(id),
    engine_version VARCHAR(64) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending', -- pending|processing|completed|failed|requires_review
    triggered_by UUID REFERENCES users(id),
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    summary_json JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE reconciliation_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reconciliation_job_id UUID NOT NULL REFERENCES reconciliation_jobs(id) ON DELETE CASCADE,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    stage_code VARCHAR(8) NOT NULL, -- A..L
    stage_name VARCHAR(128) NOT NULL,
    status VARCHAR(32) NOT NULL, -- pass|discrepancy|warning|requires_review|skipped
    invoice_line_item_id UUID REFERENCES invoice_line_items(id),
    contract_line_item_id UUID REFERENCES contract_line_items(id),
    expected_value NUMERIC(20,6),
    actual_value NUMERIC(20,6),
    variance_amount NUMERIC(20,6),
    variance_pct NUMERIC(12,6),
    permitted_tolerance TEXT, -- e.g. "2.00%" or "UNDEFINED"
    amount_outside_tolerance NUMERIC(20,6),
    currency VARCHAR(8),
    rule_applied VARCHAR(128) NOT NULL,
    calculation_details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_recon_results_job ON reconciliation_results(reconciliation_job_id);
CREATE INDEX idx_recon_results_invoice ON reconciliation_results(invoice_id);

CREATE TABLE anomaly_findings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reconciliation_job_id UUID REFERENCES reconciliation_jobs(id) ON DELETE SET NULL,
    category VARCHAR(64) NOT NULL, -- CONTRACT_PRICE_MISMATCH|QUANTITY_MISMATCH|TAX_MISMATCH|DUPLICATE_INVOICE|UNAUTHORIZED_CHARGE|PAYMENT_MISMATCH|CONTRACT_EXPIRED|DOCUMENT_MATCH_AMBIGUITY|EXTRACTION_UNCERTAIN|CURRENCY_MISMATCH|TOTAL_ARITHMETIC_MISMATCH|DUPLICATE_PAYMENT_CANDIDATE|DISCOUNT_MISMATCH
    classification_type VARCHAR(64) NOT NULL, -- exact_duplicate|probable_duplicate|potential_anomaly|confirmed_rule_violation|unresolved_discrepancy
    severity VARCHAR(32) NOT NULL, -- low|medium|high|critical (assigned strictly by configurable rule)
    severity_rule_applied VARCHAR(128) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'open', -- open|requires_review|approved|rejected|resolved|escalated
    vendor_id UUID REFERENCES vendors(id),
    invoice_id UUID REFERENCES invoices(id),
    contract_id UUID REFERENCES contracts(id),
    document_id UUID REFERENCES documents(id),
    invoice_line_item_id UUID REFERENCES invoice_line_items(id),
    expected_value NUMERIC(20,6),
    actual_value NUMERIC(20,6),
    variance_amount NUMERIC(20,6),
    currency VARCHAR(8),
    rule_applied VARCHAR(128) NOT NULL,
    explanation TEXT NOT NULL,
    calculation_details JSONB NOT NULL DEFAULT '{}'::jsonb,
    extraction_provenance JSONB NOT NULL DEFAULT '{}'::jsonb,
    reviewer_status VARCHAR(32) NOT NULL DEFAULT 'pending', -- pending|approved|rejected|resolved|escalated
    resolution_notes TEXT,
    resolved_by UUID REFERENCES users(id),
    resolved_at TIMESTAMPTZ,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_findings_status ON anomaly_findings(status);
CREATE INDEX idx_findings_category ON anomaly_findings(category);
CREATE INDEX idx_findings_invoice ON anomaly_findings(invoice_id);
CREATE INDEX idx_findings_vendor ON anomaly_findings(vendor_id);

CREATE TABLE evidence_references (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    finding_id UUID NOT NULL REFERENCES anomaly_findings(id) ON DELETE CASCADE,
    document_id UUID REFERENCES documents(id),
    extracted_field_id UUID REFERENCES extracted_fields(id),
    evidence_role VARCHAR(64) NOT NULL, -- invoice_source|contract_source|po_source|payment_source|duplicate_source
    page_number INTEGER,
    bounding_box JSONB,
    quoted_text TEXT,
    source_location TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_evidence_finding ON evidence_references(finding_id);

-- -----------------------------------------------------------------------------
-- 11. Auditable Multi-Agent Runs
-- -----------------------------------------------------------------------------
CREATE TABLE agent_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workflow_run_id UUID NOT NULL,
    step_number INTEGER NOT NULL,
    agent_name VARCHAR(64) NOT NULL, -- DOCUMENT_CLASSIFIER|CONTRACT_EXTRACTOR|INVOICE_EXTRACTOR|DOCUMENT_MATCHER|RECONCILIATION_AGENT|ANOMALY_ANALYST|AUDIT_REPORT_GENERATOR|REVIEW_COORDINATOR
    idempotency_key VARCHAR(255) NOT NULL UNIQUE,
    document_id UUID REFERENCES documents(id),
    invoice_id UUID REFERENCES invoices(id),
    status VARCHAR(32) NOT NULL, -- running|completed|failed|skipped|routed_to_human
    model_or_rule_version VARCHAR(128) NOT NULL,
    input_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    output_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    evidence_refs JSONB NOT NULL DEFAULT '[]'::jsonb,
    error_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

CREATE INDEX idx_agent_runs_workflow ON agent_runs(workflow_run_id, step_number);
CREATE INDEX idx_agent_runs_doc ON agent_runs(document_id);

-- -----------------------------------------------------------------------------
-- 12. Human-in-the-Loop Review Actions & Immutable Audit Logs
-- -----------------------------------------------------------------------------
CREATE TABLE review_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_type VARCHAR(64) NOT NULL, -- finding|document|extracted_field|invoice_match|payment_allocation
    target_id UUID NOT NULL,
    action_type VARCHAR(64) NOT NULL, -- approve|reject|correct_value|reclassify|confirm_relationship|resolve|add_explanation|escalate
    reviewer_id UUID NOT NULL REFERENCES users(id),
    previous_state JSONB NOT NULL DEFAULT '{}'::jsonb,
    new_state JSONB NOT NULL DEFAULT '{}'::jsonb,
    notes TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_review_actions_target ON review_actions(target_type, target_id);

CREATE TABLE audit_logs (
    id BIGSERIAL PRIMARY KEY,
    actor_user_id UUID REFERENCES users(id),
    actor_role VARCHAR(32),
    action VARCHAR(128) NOT NULL,
    resource_type VARCHAR(64) NOT NULL,
    resource_id TEXT,
    ip_address VARCHAR(64),
    request_id VARCHAR(64),
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_logs_created ON audit_logs(created_at DESC);
CREATE INDEX idx_audit_logs_resource ON audit_logs(resource_type, resource_id);

-- -----------------------------------------------------------------------------
-- 13. Model Metadata, Configurable Severity Rules & Audit Reports
-- -----------------------------------------------------------------------------
CREATE TABLE model_metadata (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_name VARCHAR(64) NOT NULL UNIQUE, -- local_deterministic|openai|gemini
    display_name VARCHAR(128) NOT NULL,
    model_identifier VARCHAR(128) NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    requires_external_egress BOOLEAN NOT NULL DEFAULT FALSE,
    redaction_required BOOLEAN NOT NULL DEFAULT TRUE,
    config_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_by UUID REFERENCES users(id),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO model_metadata (provider_name, display_name, model_identifier, is_enabled, is_default, requires_external_egress, redaction_required, config_json) VALUES
('local_deterministic', 'Local Deterministic + PyMuPDF/RapidOCR', 'pymupdf-1.28+rapidocr-ppocrv4+rust-rules-v1', TRUE, TRUE, FALSE, FALSE, '{"timeout_secs": 60, "data_egress": "none"}'::jsonb),
('openai', 'OpenAI Vision-Language API (Configurable)', 'unconfigured', FALSE, FALSE, TRUE, TRUE, '{"endpoint": "https://api.openai.com/v1/chat/completions", "requires_env": "OPENAI_API_KEY"}'::jsonb),
('gemini', 'Google Gemini Multimodal API (Configurable)', 'unconfigured', FALSE, FALSE, TRUE, TRUE, '{"endpoint": "https://generativelanguage.googleapis.com/v1beta/models", "requires_env": "GEMINI_API_KEY"}'::jsonb);

CREATE TABLE system_settings (
    setting_key VARCHAR(128) PRIMARY KEY,
    setting_value JSONB NOT NULL,
    description TEXT NOT NULL,
    updated_by UUID REFERENCES users(id),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO system_settings (setting_key, setting_value, description) VALUES
('severity_rules', '{"critical_variance_abs": "5000", "high_variance_abs": "1000", "medium_variance_abs": "100", "duplicate_invoice_severity": "critical", "contract_expired_severity": "high", "currency_mismatch_severity": "high", "extraction_uncertain_severity": "medium", "document_match_ambiguity_severity": "medium"}'::jsonb, 'Documented threshold rules governing finding severity assignment (never arbitrary)'),
('classification_policy', '{"min_auto_classify_confidence": 0.75, "force_classification": false}'::jsonb, 'Minimum confidence required before auto-assigning a document category'),
('data_privacy_policy', '{"enable_external_ai": false, "redact_before_external_ai": true, "preserve_original_documents": true}'::jsonb, 'Controls external AI data egress and redaction');

CREATE TABLE audit_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    report_type VARCHAR(64) NOT NULL, -- per_invoice|per_contract|per_vendor|periodic|exception|duplicate|payment
    title VARCHAR(255) NOT NULL,
    scope_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    date_from DATE,
    date_to DATE,
    rule_version VARCHAR(64) NOT NULL,
    model_version VARCHAR(128) NOT NULL,
    report_payload JSONB NOT NULL,
    generated_by UUID REFERENCES users(id),
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
