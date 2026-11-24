use crate::{
    agents::run_invoice_reconciliation_workflow,
    error::AppResult,
    reporting::build_minimal_pdf_from_lines,
    state::AppState,
};
use chrono::NaiveDate;
use rust_decimal_macros::dec;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct SyntheticFixtureSummary {
    pub label: String,
    pub vendors_created: usize,
    pub contracts_created: usize,
    pub documents_stored: usize,
    pub invoices_reconciled: usize,
    pub payments_created: usize,
}

/// Loads clearly labeled SYNTHETIC TEST FIXTURES (never real production records)
/// for deterministic end-to-end verification and UI walkthroughs.
pub async fn load_synthetic_fixtures(
    state: &AppState,
    actor_id: Uuid,
) -> AppResult<SyntheticFixtureSummary> {
    // Idempotent check
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM vendors WHERE vendor_code = 'SYNTH-VEND-001'",
    )
    .fetch_optional(&state.db)
    .await?;

    if existing.is_some() {
        let pending_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM invoices WHERE invoice_number LIKE 'SYNTH-%' AND reconciliation_status = 'pending'",
        )
        .fetch_all(&state.db)
        .await?;
        for inv_id in &pending_ids {
            run_invoice_reconciliation_workflow(state, *inv_id, Some(actor_id), None).await?;
        }
        return Ok(SyntheticFixtureSummary {
            label: "SYNTHETIC_TEST_FIXTURE (already loaded)".into(),
            vendors_created: 0,
            contracts_created: 0,
            documents_stored: 0,
            invoices_reconciled: pending_ids.len(),
            payments_created: 0,
        });
    }

    // 1. Vendors (clearly labeled synthetic)
    let v1_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO vendors (vendor_code, legal_name, tax_id, default_currency, country_code, payment_terms, metadata)
        VALUES ('SYNTH-VEND-001', '[SYNTHETIC] Northstar Cloud Infrastructure LLC', 'US-99-0001112', 'USD', 'US', 'Net 30', '{"synthetic_fixture": true}'::jsonb)
        RETURNING id
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    let v2_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO vendors (vendor_code, legal_name, tax_id, default_currency, country_code, payment_terms, metadata)
        VALUES ('SYNTH-VEND-002', '[SYNTHETIC] EuroGrid Logistics GmbH', 'DE-990022114', 'EUR', 'DE', 'Net 45', '{"synthetic_fixture": true}'::jsonb)
        RETURNING id
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    // 2. Store real generated synthetic PDF documents via content-addressed DocumentStorage
    let contract_pdf = build_minimal_pdf_from_lines(&[
        "[SYNTHETIC TEST FIXTURE] MASTER SERVICE AGREEMENT".into(),
        "Contract Number: SYNTH-CTR-2026-01".into(),
        "Contract Version: v1.0-base".into(),
        "Vendor Name: [SYNTHETIC] Northstar Cloud Infrastructure LLC".into(),
        "Effective Date: 2026-01-01".into(),
        "Expiration Date: 2026-12-31".into(),
        "Currency: USD".into(),
        "Payment Terms: Net 30".into(),
        "Price Tolerance: 2.00%".into(),
        "SKU-GPU-A100 | Dedicated GPU Compute Node | 10 unit | $1200.00 | $12000.00".into(),
        "SKU-STOR-NVME | Enterprise NVMe Storage TB | 50 unit | $45.00 | $2250.00".into(),
    ]);
    let up_c = state
        .storage
        .store_immutable("SYNTH-CTR-2026-01.pdf", "application/pdf", &contract_pdf)
        .await?;

    let contract_doc_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO documents (
            original_filename, stored_blob_key, sha256_hash, file_size_bytes,
            declared_mime_type, detected_mime_type, file_extension, page_count,
            ai_category, effective_category, processing_status, ocr_status,
            extraction_status, vendor_id, uploaded_by, is_synthetic_fixture
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,1,'contract','contract','completed','completed','completed',$8,$9,TRUE)
        RETURNING id
        "#,
    )
    .bind(&up_c.original_filename)
    .bind(&up_c.stored_blob_key)
    .bind(&up_c.sha256_hash)
    .bind(up_c.file_size_bytes)
    .bind(&up_c.declared_mime)
    .bind(&up_c.detected_mime)
    .bind(&up_c.extension)
    .bind(v1_id)
    .bind(actor_id)
    .fetch_one(&state.db)
    .await?;

    // 3. Contract + Version 1 (Q1-Q2) + Amendment v2 (Q3-Q4 with updated rate)
    let c1_id: Uuid = sqlx::query_scalar(
        "INSERT INTO contracts (contract_number, vendor_id, title, currency, status) VALUES ('SYNTH-CTR-2026-01', $1, '[SYNTHETIC] Cloud Infrastructure MSA 2026', 'USD', 'active') RETURNING id",
    )
    .bind(v1_id)
    .fetch_one(&state.db)
    .await?;

    let cv1_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO contract_versions (
            contract_id, document_id, version_label, version_number, is_amendment,
            effective_date, expiration_date, payment_terms, price_tolerance_pct,
            default_tax_rate, tax_provision_notes, allowed_additional_charges
        )
        VALUES ($1,$2,'v1.0-base',1,FALSE,'2026-01-01','2026-06-30','Net 30',2.00,8.00,'Documented 8% cloud services tax','["Data Egress"]'::jsonb)
        RETURNING id
        "#,
    )
    .bind(c1_id)
    .bind(contract_doc_id)
    .fetch_one(&state.db)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO contract_line_items (contract_version_id, item_code, description, unit, unit_price, currency, discount_pct, tax_rate)
        VALUES
          ($1, 'SKU-GPU-A100', 'Dedicated GPU Compute Node', 'unit', 1200.00, 'USD', 5.00, 8.00),
          ($1, 'SKU-STOR-NVME', 'Enterprise NVMe Storage TB', 'unit', 45.00, 'USD', 0.00, 8.00)
        "#,
    )
    .bind(cv1_id)
    .execute(&state.db)
    .await?;

    let cv2_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO contract_versions (
            contract_id, document_id, version_label, version_number, is_amendment, amends_version_id,
            effective_date, expiration_date, payment_terms, price_tolerance_pct,
            default_tax_rate, tax_provision_notes, allowed_additional_charges
        )
        VALUES ($1,$2,'v2.0-amendment-q3',2,TRUE,$3,'2026-07-01','2026-12-31','Net 30',NULL,8.00,'Amendment v2: reduced GPU rate, tolerance policy intentionally undefined','["Data Egress"]'::jsonb)
        RETURNING id
        "#,
    )
    .bind(c1_id)
    .bind(contract_doc_id)
    .bind(cv1_id)
    .fetch_one(&state.db)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO contract_line_items (contract_version_id, item_code, description, unit, unit_price, currency, discount_pct, tax_rate)
        VALUES
          ($1, 'SKU-GPU-A100', 'Dedicated GPU Compute Node', 'unit', 1100.00, 'USD', 5.00, 8.00),
          ($1, 'SKU-STOR-NVME', 'Enterprise NVMe Storage TB', 'unit', 42.00, 'USD', 0.00, 8.00)
        "#,
    )
    .bind(cv2_id)
    .execute(&state.db)
    .await?;

    // Vendor 2 Contract (EUR)
    let c2_id: Uuid = sqlx::query_scalar(
        "INSERT INTO contracts (contract_number, vendor_id, title, currency, status) VALUES ('SYNTH-CTR-EU-2026', $1, '[SYNTHETIC] European Freight Agreement', 'EUR', 'active') RETURNING id",
    )
    .bind(v2_id)
    .fetch_one(&state.db)
    .await?;

    let cv_eu_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO contract_versions (
            contract_id, version_label, version_number, is_amendment,
            effective_date, expiration_date, payment_terms, default_tax_rate
        )
        VALUES ($1,'v1.0-eu',1,FALSE,'2026-01-01','2026-09-30','Net 45',19.00)
        RETURNING id
        "#,
    )
    .bind(c2_id)
    .fetch_one(&state.db)
    .await?;

    sqlx::query(
        "INSERT INTO contract_line_items (contract_version_id, item_code, description, unit, unit_price, currency, discount_pct, tax_rate) VALUES ($1, 'EU-PALLET', 'Cross-Border Pallet Freight', 'unit', 250.00, 'EUR', 0.00, 19.00)",
    )
    .bind(cv_eu_id)
    .execute(&state.db)
    .await?;

    // 4. Purchase Order
    let po1_id: Uuid = sqlx::query_scalar(
        "INSERT INTO purchase_orders (po_number, vendor_id, contract_id, issue_date, currency, total_amount) VALUES ('SYNTH-PO-8801', $1, $2, '2026-08-01', 'USD', 14353.20) RETURNING id",
    )
    .bind(v1_id)
    .bind(c1_id)
    .fetch_one(&state.db)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO purchase_order_line_items (purchase_order_id, item_code, description, unit, ordered_quantity, received_quantity, unit_price, line_total)
        VALUES
          ($1, 'SKU-GPU-A100', 'Dedicated GPU Compute Node', 'unit', 10, 10, 1100.00, 11000.00),
          ($1, 'SKU-STOR-NVME', 'Enterprise NVMe Storage TB', 'unit', 50, 50, 42.00, 2100.00)
        "#,
    )
    .bind(po1_id)
    .execute(&state.db)
    .await?;

    // Helper to create invoice PDF + document row
    let inv_discrepancy_pdf = build_minimal_pdf_from_lines(&[
        "[SYNTHETIC TEST FIXTURE] TAX INVOICE".into(),
        "Invoice Number: SYNTH-INV-2026-1002".into(),
        "Invoice Date: 2026-08-15".into(),
        "Vendor Name: [SYNTHETIC] Northstar Cloud Infrastructure LLC".into(),
        "Contract Ref: SYNTH-CTR-2026-01".into(),
        "PO Number: SYNTH-PO-8801".into(),
        "Currency: USD".into(),
        "SKU-GPU-A100 | Dedicated GPU Compute Node | 12 unit | $1250.00 | $14250.00".into(),
        "SKU-STOR-NVME | Enterprise NVMe Storage TB | 50 unit | $42.00 | $2100.00".into(),
        "SURCHARGE-EXP | Emergency Expedite Fee | 1 unit | $850.00 | $850.00".into(),
        "Subtotal: USD 16350.00".into(),
        "Tax Amount: USD 1496.00".into(),
        "Total Amount: USD 18696.00".into(),
    ]);
    let up_inv = state
        .storage
        .store_immutable("SYNTH-INV-2026-1002.pdf", "application/pdf", &inv_discrepancy_pdf)
        .await?;
    let inv_doc_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO documents (
            original_filename, stored_blob_key, sha256_hash, file_size_bytes,
            declared_mime_type, detected_mime_type, file_extension, page_count,
            ai_category, effective_category, processing_status, ocr_status,
            extraction_status, vendor_id, uploaded_by, is_synthetic_fixture
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,1,'invoice','invoice','completed','completed','completed',$8,$9,TRUE)
        RETURNING id
        "#,
    )
    .bind(&up_inv.original_filename)
    .bind(&up_inv.stored_blob_key)
    .bind(&up_inv.sha256_hash)
    .bind(up_inv.file_size_bytes)
    .bind(&up_inv.declared_mime)
    .bind(&up_inv.detected_mime)
    .bind(&up_inv.extension)
    .bind(v1_id)
    .bind(actor_id)
    .fetch_one(&state.db)
    .await?;

    // 5A. Clean Reconciled Invoice (uses Amendment v2 pricing & 5% discount & 8% tax)
    // 10 * 1100 = 11000 - 5% (550) = 10450; tax 8% = 836.00; 50 * 42 = 2100; tax 8% = 168.00; total = 13554.00
    let inv1_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO invoices (
            invoice_number, vendor_id, customer_name, invoice_date, due_date, currency,
            referenced_contract_number, referenced_po_number,
            stated_subtotal, stated_discount_total, stated_tax_total, stated_additional_charges, stated_total, payment_terms
        )
        VALUES ('SYNTH-INV-2026-1001', $1, 'ABIFR Test Corp', '2026-08-05', '2026-09-04', 'USD',
                'SYNTH-CTR-2026-01', 'SYNTH-PO-8801', 12550.00, 550.00, 1004.00, 0.00, 13554.00, 'Net 30')
        RETURNING id
        "#,
    )
    .bind(v1_id)
    .fetch_one(&state.db)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO invoice_line_items (invoice_id, line_number, item_code, description, quantity, unit, unit_price, discount_pct, discount_amount, tax_rate, tax_amount, stated_line_subtotal, stated_line_total, source_page)
        VALUES
          ($1, 1, 'SKU-GPU-A100', 'Dedicated GPU Compute Node', 10, 'unit', 1100.00, 5.00, 550.00, 8.00, 836.00, 10450.00, 11286.00, 1),
          ($1, 2, 'SKU-STOR-NVME', 'Enterprise NVMe Storage TB', 50, 'unit', 42.00, 0.00, 0.00, 8.00, 168.00, 2100.00, 2268.00, 1)
        "#,
    )
    .bind(inv1_id)
    .execute(&state.db)
    .await?;

    // 5B. Multi-Discrepancy Invoice (price mismatch vs Amendment v2: $1250 vs $1100, qty mismatch: 12 vs PO 10, tax overcharge, unauthorized expedite charge $850)
    let inv2_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO invoices (
            document_id, invoice_number, vendor_id, customer_name, invoice_date, due_date, currency,
            referenced_contract_number, referenced_po_number,
            stated_subtotal, stated_discount_total, stated_tax_total, stated_additional_charges, stated_total, payment_terms
        )
        VALUES ($1, 'SYNTH-INV-2026-1002', $2, 'ABIFR Test Corp', '2026-08-15', '2026-09-14', 'USD',
                'SYNTH-CTR-2026-01', 'SYNTH-PO-8801', 16350.00, 750.00, 1496.00, 850.00, 18696.00, 'Net 30')
        RETURNING id
        "#,
    )
    .bind(inv_doc_id)
    .bind(v1_id)
    .fetch_one(&state.db)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO invoice_line_items (invoice_id, line_number, item_code, description, quantity, unit, unit_price, discount_pct, discount_amount, tax_rate, tax_amount, stated_line_subtotal, stated_line_total, is_additional_charge, source_page)
        VALUES
          ($1, 1, 'SKU-GPU-A100', 'Dedicated GPU Compute Node', 12, 'unit', 1250.00, 5.00, 750.00, 8.00, 1260.00, 14250.00, 15510.00, FALSE, 1),
          ($1, 2, 'SKU-STOR-NVME', 'Enterprise NVMe Storage TB', 50, 'unit', 42.00, 0.00, 0.00, 8.00, 168.00, 2100.00, 2268.00, FALSE, 1),
          ($1, 3, 'SURCHARGE-EXP', 'Emergency Expedite Fee', 1, 'unit', 850.00, 0.00, 0.00, 8.00, 68.00, 850.00, 918.00, TRUE, 1)
        "#,
    )
    .bind(inv2_id)
    .execute(&state.db)
    .await?;

    // 5C. Expired Contract + EUR Currency Scenario (Invoice dated 2026-11-10 after 2026-09-30 expiry)
    let inv3_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO invoices (
            invoice_number, vendor_id, customer_name, invoice_date, currency,
            referenced_contract_number,
            stated_subtotal, stated_discount_total, stated_tax_total, stated_additional_charges, stated_total
        )
        VALUES ('SYNTH-INV-EU-9001', $1, 'ABIFR Test Corp', '2026-11-10', 'EUR',
                'SYNTH-CTR-EU-2026', 2500.00, 0.00, 475.00, 0.00, 2975.00)
        RETURNING id
        "#,
    )
    .bind(v2_id)
    .fetch_one(&state.db)
    .await?;

    sqlx::query(
        "INSERT INTO invoice_line_items (invoice_id, line_number, item_code, description, quantity, unit, unit_price, tax_rate, tax_amount, stated_line_subtotal, stated_line_total, source_page) VALUES ($1, 1, 'EU-PALLET', 'Cross-Border Pallet Freight', 10, 'unit', 250.00, 19.00, 475.00, 2500.00, 2975.00, 1)",
    )
    .bind(inv3_id)
    .execute(&state.db)
    .await?;

    // 5D. Duplicate Invoice Number Fixture
    let inv4_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO invoices (
            invoice_number, vendor_id, customer_name, invoice_date, currency,
            referenced_contract_number,
            stated_subtotal, stated_discount_total, stated_tax_total, stated_additional_charges, stated_total
        )
        VALUES ('SYNTH-INV-2026-1001', $1, 'ABIFR Test Corp', '2026-08-06', 'USD',
                'SYNTH-CTR-2026-01', 12550.00, 550.00, 1004.00, 0.00, 13554.00)
        RETURNING id
        "#,
    )
    .bind(v1_id)
    .fetch_one(&state.db)
    .await?;

    sqlx::query(
        "INSERT INTO invoice_line_items (invoice_id, line_number, item_code, description, quantity, unit, unit_price, discount_pct, discount_amount, tax_rate, tax_amount, stated_line_subtotal, stated_line_total, source_page) VALUES ($1, 1, 'SKU-GPU-A100', 'Dedicated GPU Compute Node', 10, 'unit', 1100.00, 5.00, 550.00, 8.00, 836.00, 10450.00, 11286.00, 1), ($1, 2, 'SKU-STOR-NVME', 'Enterprise NVMe Storage TB', 50, 'unit', 42.00, 0.00, 0.00, 8.00, 168.00, 2100.00, 2268.00, 1)",
    )
    .bind(inv4_id)
    .execute(&state.db)
    .await?;

    // 6. Payments: Full payment for inv1, overpayment + duplicate payment candidate for inv2, unmatched payment preserved
    let p1_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO payments (payment_reference, bank_reference, vendor_id, beneficiary_name, payment_date, amount, currency, referenced_invoice_number, transaction_status, allocation_status)
        VALUES ('SYNTH-PAY-5001', 'WIRE-0019283', $1, '[SYNTHETIC] Northstar Cloud Infrastructure LLC', '2026-08-20', 13554.00, 'USD', 'SYNTH-INV-2026-1001', 'settled', 'fully_allocated')
        RETURNING id
        "#,
    )
    .bind(v1_id)
    .fetch_one(&state.db)
    .await?;

    sqlx::query(
        "INSERT INTO payment_allocations (payment_id, invoice_id, allocated_amount, currency, matching_evidence) VALUES ($1, $2, $3, 'USD', 'Explicit invoice_ref SYNTH-INV-2026-1001 + vendor + currency')",
    )
    .bind(p1_id)
    .bind(inv1_id)
    .bind(dec!(13554.00))
    .execute(&state.db)
    .await?;

    // Duplicate/Overpayment candidate on inv2
    sqlx::query(
        r#"
        INSERT INTO payments (payment_reference, bank_reference, vendor_id, beneficiary_name, payment_date, amount, currency, referenced_invoice_number, transaction_status, allocation_status)
        VALUES
          ('SYNTH-PAY-5002A', 'WIRE-0019401', $1, '[SYNTHETIC] Northstar Cloud Infrastructure LLC', $2, 18696.00, 'USD', 'SYNTH-INV-2026-1002', 'settled', 'requires_review'),
          ('SYNTH-PAY-5002B', 'WIRE-0019402', $1, '[SYNTHETIC] Northstar Cloud Infrastructure LLC', $2, 18696.00, 'USD', 'SYNTH-INV-2026-1002', 'settled', 'requires_review'),
          ('SYNTH-PAY-UNMATCHED', 'WIRE-0019999', $1, '[SYNTHETIC] Northstar Cloud Infrastructure LLC', $2, 4200.00, 'USD', NULL, 'settled', 'unmatched')
        "#,
    )
    .bind(v1_id)
    .bind(NaiveDate::from_ymd_opt(2026, 8, 25).unwrap())
    .execute(&state.db)
    .await?;

    // 7. Run deterministic reconciliation workflow on all 4 synthetic invoices!
    for inv_id in [inv1_id, inv2_id, inv3_id, inv4_id] {
        run_invoice_reconciliation_workflow(state, inv_id, Some(actor_id), None).await?;
    }

    Ok(SyntheticFixtureSummary {
        label: "SYNTHETIC_TEST_FIXTURE".into(),
        vendors_created: 2,
        contracts_created: 2,
        documents_stored: 2,
        invoices_reconciled: 4,
        payments_created: 4,
    })
}
