-- Widen lifecycle/match status columns to VARCHAR(64) to accommodate descriptive audit statuses
ALTER TABLE invoices ALTER COLUMN match_confidence_status TYPE VARCHAR(64);
ALTER TABLE invoices ALTER COLUMN reconciliation_status TYPE VARCHAR(64);
ALTER TABLE documents ALTER COLUMN matching_status TYPE VARCHAR(64);
ALTER TABLE documents ALTER COLUMN reconciliation_status TYPE VARCHAR(64);
ALTER TABLE reconciliation_jobs ALTER COLUMN status TYPE VARCHAR(64);
