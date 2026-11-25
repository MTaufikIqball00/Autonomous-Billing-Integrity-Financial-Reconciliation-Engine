use chrono::NaiveDate;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const RECONCILIATION_ENGINE_VERSION: &str = "abifr-deterministic-engine-v1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityRules {
    pub critical_variance_abs: Decimal,
    pub high_variance_abs: Decimal,
    pub medium_variance_abs: Decimal,
    pub duplicate_invoice_severity: String,
    pub contract_expired_severity: String,
    pub currency_mismatch_severity: String,
    pub extraction_uncertain_severity: String,
    pub document_match_ambiguity_severity: String,
}

impl Default for SeverityRules {
    fn default() -> Self {
        Self {
            critical_variance_abs: dec!(5000),
            high_variance_abs: dec!(1000),
            medium_variance_abs: dec!(100),
            duplicate_invoice_severity: "critical".into(),
            contract_expired_severity: "high".into(),
            currency_mismatch_severity: "high".into(),
            extraction_uncertain_severity: "medium".into(),
            document_match_ambiguity_severity: "medium".into(),
        }
    }
}

impl SeverityRules {
    pub fn severity_for_variance(&self, abs_variance: Decimal) -> (&'static str, &'static str) {
        if abs_variance >= self.critical_variance_abs {
            ("critical", "SEVERITY_RULE_VARIANCE_CRITICAL")
        } else if abs_variance >= self.high_variance_abs {
            ("high", "SEVERITY_RULE_VARIANCE_HIGH")
        } else if abs_variance >= self.medium_variance_abs {
            ("medium", "SEVERITY_RULE_VARIANCE_MEDIUM")
        } else {
            ("low", "SEVERITY_RULE_VARIANCE_LOW")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractVersionInput {
    pub version_id: Uuid,
    pub contract_id: Uuid,
    pub contract_number: String,
    pub vendor_id: Uuid,
    pub currency: String,
    pub version_label: String,
    pub version_number: i32,
    pub is_amendment: bool,
    pub effective_date: NaiveDate,
    pub expiration_date: NaiveDate,
    pub price_tolerance_pct: Option<Decimal>,
    pub price_tolerance_abs: Option<Decimal>,
    pub quantity_tolerance_pct: Option<Decimal>,
    pub default_tax_rate: Option<Decimal>,
    pub allowed_additional_charges: Vec<String>,
    pub document_id: Option<Uuid>,
    pub line_items: Vec<ContractLineItemInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractLineItemInput {
    pub id: Uuid,
    pub item_code: Option<String>,
    pub description: String,
    pub unit: String,
    pub unit_price: Decimal,
    pub currency: String,
    pub min_quantity: Option<Decimal>,
    pub max_quantity: Option<Decimal>,
    pub discount_pct: Decimal,
    pub tax_rate: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseOrderInput {
    pub id: Uuid,
    pub po_number: String,
    pub vendor_id: Uuid,
    pub currency: String,
    pub total_amount: Decimal,
    pub document_id: Option<Uuid>,
    pub line_items: Vec<PoLineItemInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoLineItemInput {
    pub id: Uuid,
    pub item_code: Option<String>,
    pub description: String,
    pub unit: String,
    pub ordered_quantity: Decimal,
    pub received_quantity: Option<Decimal>,
    pub unit_price: Decimal,
    pub line_total: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceInput {
    pub id: Uuid,
    pub document_id: Option<Uuid>,
    pub invoice_number: String,
    pub vendor_id: Uuid,
    pub invoice_date: NaiveDate,
    pub currency: String,
    pub referenced_contract_number: Option<String>,
    pub referenced_po_number: Option<String>,
    pub stated_subtotal: Decimal,
    pub stated_discount_total: Decimal,
    pub stated_tax_total: Decimal,
    pub stated_additional_charges: Decimal,
    pub stated_total: Decimal,
    pub is_credit_note: bool,
    pub line_items: Vec<InvoiceLineItemInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceLineItemInput {
    pub id: Uuid,
    pub line_number: i32,
    pub item_code: Option<String>,
    pub description: String,
    pub quantity: Decimal,
    pub unit: String,
    pub unit_price: Decimal,
    pub discount_pct: Decimal,
    pub discount_amount: Decimal,
    pub tax_rate: Option<Decimal>,
    pub tax_amount: Decimal,
    pub stated_line_subtotal: Decimal,
    pub stated_line_total: Decimal,
    pub is_additional_charge: bool,
    pub human_confirmed_contract_item_id: Option<Uuid>,
    pub source_page: Option<i32>,
    pub source_bbox: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingInvoiceSummary {
    pub id: Uuid,
    pub document_id: Option<Uuid>,
    pub sha256_hash: Option<String>,
    pub invoice_number: String,
    pub vendor_id: Uuid,
    pub invoice_date: NaiveDate,
    pub currency: String,
    pub stated_total: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentInput {
    pub id: Uuid,
    pub document_id: Option<Uuid>,
    pub payment_reference: String,
    pub bank_reference: Option<String>,
    pub vendor_id: Uuid,
    pub payment_date: NaiveDate,
    pub amount: Decimal,
    pub currency: String,
    pub referenced_invoice_number: Option<String>,
    pub transaction_status: String,
    pub is_refund_or_credit: bool,
    /// Pre-existing explicit allocations (for one payment covering multiple invoices)
    pub explicit_allocated_to_this_invoice: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    pub stage_code: String,
    pub stage_name: String,
    pub status: String,
    pub invoice_line_item_id: Option<Uuid>,
    pub contract_line_item_id: Option<Uuid>,
    pub expected_value: Option<Decimal>,
    pub actual_value: Option<Decimal>,
    pub variance_amount: Option<Decimal>,
    pub variance_pct: Option<Decimal>,
    pub permitted_tolerance: Option<String>,
    pub amount_outside_tolerance: Option<Decimal>,
    pub currency: Option<String>,
    pub rule_applied: String,
    pub calculation_details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRefOutput {
    pub document_id: Option<Uuid>,
    pub evidence_role: String,
    pub page_number: Option<i32>,
    pub bounding_box: Option<serde_json::Value>,
    pub quoted_text: Option<String>,
    pub source_location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingOutput {
    pub category: String,
    pub classification_type: String,
    pub severity: String,
    pub severity_rule_applied: String,
    pub status: String,
    pub invoice_line_item_id: Option<Uuid>,
    pub contract_id: Option<Uuid>,
    pub expected_value: Option<Decimal>,
    pub actual_value: Option<Decimal>,
    pub variance_amount: Option<Decimal>,
    pub currency: Option<String>,
    pub rule_applied: String,
    pub explanation: String,
    pub calculation_details: serde_json::Value,
    pub evidence_refs: Vec<EvidenceRefOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineMatchDecision {
    pub invoice_line_item_id: Uuid,
    pub matched_contract_item_id: Option<Uuid>,
    pub matched_po_item_id: Option<Uuid>,
    pub match_method: String,
    pub match_requires_review: bool,
    pub candidate_score: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationOutput {
    pub engine_version: String,
    pub overall_status: String,
    pub matched_contract_id: Option<Uuid>,
    pub matched_contract_version_id: Option<Uuid>,
    pub matched_po_id: Option<Uuid>,
    pub match_confidence_status: String,
    pub recalculated_subtotal: Decimal,
    pub recalculated_discount_total: Decimal,
    pub recalculated_tax_total: Decimal,
    pub recalculated_additional_charges: Decimal,
    pub recalculated_total: Decimal,
    pub total_allocated_payments: Decimal,
    pub payment_status: String,
    pub line_matches: Vec<LineMatchDecision>,
    pub stages: Vec<StageResult>,
    pub findings: Vec<FindingOutput>,
}
