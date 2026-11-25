use super::{pagination_json, AuthenticatedUser, PaginationQuery};
use crate::{
    agents::run_invoice_reconciliation_workflow,
    error::{AppError, AppResult},
    security::require_permission,
    state::{parse_uuid, AppState},
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

// =============================================================================
// Vendors
// =============================================================================
pub async fn list_vendors(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(pg): Query<PaginationQuery>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let (limit, offset) = pg.resolve();
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vendors")
        .fetch_one(&state.db)
        .await?;

    let rows = sqlx::query(
        r#"
        SELECT v.id, v.vendor_code, v.legal_name, v.tax_id, v.default_currency,
               v.country_code, v.payment_terms, v.status, v.created_at,
               (SELECT COUNT(*) FROM contracts c WHERE c.vendor_id = v.id) AS contract_count,
               (SELECT COUNT(*) FROM invoices i WHERE i.vendor_id = v.id) AS invoice_count,
               (SELECT COUNT(*) FROM anomaly_findings f WHERE f.vendor_id = v.id AND f.reviewer_status = 'pending') AS open_findings
        FROM vendors v
        ORDER BY v.legal_name ASC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let vendors: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "vendor_code": r.get::<String, _>("vendor_code"),
                "legal_name": r.get::<String, _>("legal_name"),
                "tax_id": r.get::<Option<String>, _>("tax_id"),
                "default_currency": r.get::<Option<String>, _>("default_currency"),
                "country_code": r.get::<Option<String>, _>("country_code"),
                "payment_terms": r.get::<Option<String>, _>("payment_terms"),
                "status": r.get::<String, _>("status"),
                "contract_count": r.get::<i64, _>("contract_count"),
                "invoice_count": r.get::<i64, _>("invoice_count"),
                "open_findings": r.get::<i64, _>("open_findings")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({
        "vendors": vendors,
        "pagination": pagination_json(total, limit, offset)
    })))
}

#[derive(Deserialize)]
pub struct CreateVendorRequest {
    pub vendor_code: String,
    pub legal_name: String,
    pub tax_id: Option<String>,
    pub default_currency: Option<String>,
    pub country_code: Option<String>,
    pub payment_terms: Option<String>,
}

pub async fn create_vendor(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<CreateVendorRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "vendors:write")?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO vendors (vendor_code, legal_name, tax_id, default_currency, country_code, payment_terms) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(req.vendor_code.trim())
    .bind(req.legal_name.trim())
    .bind(req.tax_id)
    .bind(req.default_currency)
    .bind(req.country_code)
    .bind(req.payment_terms)
    .fetch_one(&state.db)
    .await?;

    state
        .log_audit(
            Some(claims.sub),
            Some(&claims.role),
            "vendor.created",
            "vendor",
            Some(&id.to_string()),
            serde_json::json!({ "vendor_code": req.vendor_code }),
        )
        .await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

#[derive(Deserialize)]
pub struct UpdateVendorRequest {
    pub legal_name: Option<String>,
    pub tax_id: Option<String>,
    pub default_currency: Option<String>,
    pub country_code: Option<String>,
    pub payment_terms: Option<String>,
    pub status: Option<String>,
}

pub async fn update_vendor(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateVendorRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "vendors:write")?;
    let vid = parse_uuid(&id, "vendor_id")?;

    sqlx::query(
        r#"
        UPDATE vendors
        SET legal_name = COALESCE($2, legal_name),
            tax_id = COALESCE($3, tax_id),
            default_currency = COALESCE($4, default_currency),
            country_code = COALESCE($5, country_code),
            payment_terms = COALESCE($6, payment_terms),
            status = COALESCE($7, status),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(vid)
    .bind(req.legal_name.as_deref().map(str::trim))
    .bind(&req.tax_id)
    .bind(req.default_currency.as_deref().map(str::trim))
    .bind(&req.country_code)
    .bind(&req.payment_terms)
    .bind(&req.status)
    .execute(&state.db)
    .await?;

    state
        .log_audit(
            Some(claims.sub),
            Some(&claims.role),
            "vendor.updated",
            "vendor",
            Some(&vid.to_string()),
            serde_json::json!({ "status": req.status }),
        )
        .await;

    Ok(Json(serde_json::json!({ "status": "updated", "id": vid })))
}

// =============================================================================
// Contracts & Amendments
// =============================================================================
pub async fn list_contracts(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(pg): Query<PaginationQuery>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let (limit, offset) = pg.resolve();
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contracts")
        .fetch_one(&state.db)
        .await?;

    let rows = sqlx::query(
        r#"
        SELECT c.id, c.contract_number, c.title, c.currency, c.status, c.vendor_id, v.legal_name AS vendor_name,
               (SELECT COUNT(*) FROM contract_versions cv WHERE cv.contract_id = c.id) AS version_count,
               (SELECT MAX(cv.expiration_date) FROM contract_versions cv WHERE cv.contract_id = c.id) AS latest_expiration
        FROM contracts c
        JOIN vendors v ON v.id = c.vendor_id
        ORDER BY c.created_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let contracts: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "contract_number": r.get::<String, _>("contract_number"),
                "title": r.get::<String, _>("title"),
                "currency": r.get::<String, _>("currency"),
                "status": r.get::<String, _>("status"),
                "vendor_id": r.get::<Uuid, _>("vendor_id"),
                "vendor_name": r.get::<String, _>("vendor_name"),
                "version_count": r.get::<i64, _>("version_count"),
                "latest_expiration": r.get::<Option<NaiveDate>, _>("latest_expiration")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({
        "contracts": contracts,
        "pagination": pagination_json(total, limit, offset)
    })))
}

#[derive(Deserialize)]
pub struct ContractLineItemReq {
    pub item_code: Option<String>,
    pub description: String,
    pub unit: String,
    pub unit_price: Decimal,
    pub discount_pct: Option<Decimal>,
    pub tax_rate: Option<Decimal>,
}

#[derive(Deserialize)]
pub struct CreateContractRequest {
    pub contract_number: String,
    pub vendor_id: Uuid,
    pub title: String,
    pub currency: String,
    pub effective_date: NaiveDate,
    pub expiration_date: NaiveDate,
    pub payment_terms: Option<String>,
    pub price_tolerance_pct: Option<Decimal>,
    pub price_tolerance_abs: Option<Decimal>,
    pub default_tax_rate: Option<Decimal>,
    pub allowed_additional_charges: Option<Vec<String>>,
    pub line_items: Vec<ContractLineItemReq>,
}

pub async fn create_contract(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<CreateContractRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "contracts:write")?;
    let mut tx = state.db.begin().await?;

    let contract_id: Uuid = sqlx::query_scalar(
        "INSERT INTO contracts (contract_number, vendor_id, title, currency) VALUES ($1,$2,$3,$4) RETURNING id",
    )
    .bind(req.contract_number.trim())
    .bind(req.vendor_id)
    .bind(req.title.trim())
    .bind(req.currency.trim().to_ascii_uppercase())
    .fetch_one(&mut *tx)
    .await?;

    let version_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO contract_versions (
            contract_id, version_label, version_number, is_amendment,
            effective_date, expiration_date, payment_terms,
            price_tolerance_pct, price_tolerance_abs, default_tax_rate, allowed_additional_charges
        )
        VALUES ($1,'v1.0',1,FALSE,$2,$3,$4,$5,$6,$7,$8)
        RETURNING id
        "#,
    )
    .bind(contract_id)
    .bind(req.effective_date)
    .bind(req.expiration_date)
    .bind(&req.payment_terms)
    .bind(req.price_tolerance_pct)
    .bind(req.price_tolerance_abs)
    .bind(req.default_tax_rate)
    .bind(serde_json::to_value(req.allowed_additional_charges.unwrap_or_default()).unwrap())
    .fetch_one(&mut *tx)
    .await?;

    for item in &req.line_items {
        sqlx::query(
            "INSERT INTO contract_line_items (contract_version_id, item_code, description, unit, unit_price, currency, discount_pct, tax_rate) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(version_id)
        .bind(&item.item_code)
        .bind(&item.description)
        .bind(&item.unit)
        .bind(item.unit_price)
        .bind(req.currency.trim().to_ascii_uppercase())
        .bind(item.discount_pct.unwrap_or(Decimal::ZERO))
        .bind(item.tax_rate)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": contract_id, "version_id": version_id }))))
}

pub async fn get_contract_detail(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let cid = parse_uuid(&id, "contract_id")?;

    let c = sqlx::query(
        "SELECT c.id, c.contract_number, c.title, c.currency, c.status, c.vendor_id, v.legal_name AS vendor_name FROM contracts c JOIN vendors v ON v.id = c.vendor_id WHERE c.id = $1",
    )
    .bind(cid)
    .fetch_one(&state.db)
    .await?;

    let v_rows = sqlx::query(
        r#"
        SELECT id, version_label, version_number, is_amendment, effective_date, expiration_date,
               payment_terms, price_tolerance_pct, price_tolerance_abs, quantity_tolerance_pct,
               default_tax_rate, tax_provision_notes, allowed_additional_charges, document_id
        FROM contract_versions WHERE contract_id = $1 ORDER BY version_number ASC
        "#,
    )
    .bind(cid)
    .fetch_all(&state.db)
    .await?;

    let mut versions = Vec::new();
    for vr in v_rows {
        let vid: Uuid = vr.get("id");
        let item_rows = sqlx::query(
            "SELECT id, item_code, description, unit, unit_price, currency, discount_pct, tax_rate FROM contract_line_items WHERE contract_version_id = $1",
        )
        .bind(vid)
        .fetch_all(&state.db)
        .await?;

        let items: Vec<serde_json::Value> = item_rows
            .into_iter()
            .map(|ir| {
                serde_json::json!({
                    "id": ir.get::<Uuid, _>("id"),
                    "item_code": ir.get::<Option<String>, _>("item_code"),
                    "description": ir.get::<String, _>("description"),
                    "unit": ir.get::<String, _>("unit"),
                    "unit_price": ir.get::<Decimal, _>("unit_price"),
                    "currency": ir.get::<String, _>("currency"),
                    "discount_pct": ir.get::<Option<Decimal>, _>("discount_pct"),
                    "tax_rate": ir.get::<Option<Decimal>, _>("tax_rate")
                })
            })
            .collect();

        versions.push(serde_json::json!({
            "id": vid,
            "version_label": vr.get::<String, _>("version_label"),
            "version_number": vr.get::<i32, _>("version_number"),
            "is_amendment": vr.get::<bool, _>("is_amendment"),
            "effective_date": vr.get::<NaiveDate, _>("effective_date"),
            "expiration_date": vr.get::<NaiveDate, _>("expiration_date"),
            "payment_terms": vr.get::<Option<String>, _>("payment_terms"),
            "price_tolerance_pct": vr.get::<Option<Decimal>, _>("price_tolerance_pct"),
            "price_tolerance_abs": vr.get::<Option<Decimal>, _>("price_tolerance_abs"),
            "default_tax_rate": vr.get::<Option<Decimal>, _>("default_tax_rate"),
            "tax_provision_notes": vr.get::<Option<String>, _>("tax_provision_notes"),
            "allowed_additional_charges": vr.get::<serde_json::Value, _>("allowed_additional_charges"),
            "document_id": vr.get::<Option<Uuid>, _>("document_id"),
            "line_items": items
        }));
    }

    Ok(Json(serde_json::json!({
        "contract": {
            "id": c.get::<Uuid, _>("id"),
            "contract_number": c.get::<String, _>("contract_number"),
            "title": c.get::<String, _>("title"),
            "currency": c.get::<String, _>("currency"),
            "status": c.get::<String, _>("status"),
            "vendor_id": c.get::<Uuid, _>("vendor_id"),
            "vendor_name": c.get::<String, _>("vendor_name")
        },
        "versions": versions
    })))
}

#[derive(Deserialize)]
pub struct AddContractVersionRequest {
    pub version_label: String,
    pub effective_date: NaiveDate,
    pub expiration_date: NaiveDate,
    pub price_tolerance_pct: Option<Decimal>,
    pub default_tax_rate: Option<Decimal>,
    pub line_items: Vec<ContractLineItemReq>,
}

pub async fn add_contract_version(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<AddContractVersionRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "contracts:write")?;
    let cid = parse_uuid(&id, "contract_id")?;

    let currency: String = sqlx::query_scalar("SELECT currency FROM contracts WHERE id = $1")
        .bind(cid)
        .fetch_one(&state.db)
        .await?;
    let next_ver: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM contract_versions WHERE contract_id = $1",
    )
    .bind(cid)
    .fetch_one(&state.db)
    .await?;

    let mut tx = state.db.begin().await?;
    let vid: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO contract_versions (contract_id, version_label, version_number, is_amendment, effective_date, expiration_date, price_tolerance_pct, default_tax_rate)
        VALUES ($1,$2,$3,TRUE,$4,$5,$6,$7) RETURNING id
        "#,
    )
    .bind(cid)
    .bind(&req.version_label)
    .bind(next_ver)
    .bind(req.effective_date)
    .bind(req.expiration_date)
    .bind(req.price_tolerance_pct)
    .bind(req.default_tax_rate)
    .fetch_one(&mut *tx)
    .await?;

    for item in &req.line_items {
        sqlx::query(
            "INSERT INTO contract_line_items (contract_version_id, item_code, description, unit, unit_price, currency, discount_pct, tax_rate) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(vid)
        .bind(&item.item_code)
        .bind(&item.description)
        .bind(&item.unit)
        .bind(item.unit_price)
        .bind(&currency)
        .bind(item.discount_pct.unwrap_or(Decimal::ZERO))
        .bind(item.tax_rate)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "version_id": vid, "version_number": next_ver }))))
}

// =============================================================================
// Purchase Orders
// =============================================================================
pub async fn list_purchase_orders(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(pg): Query<PaginationQuery>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let (limit, offset) = pg.resolve();
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM purchase_orders")
        .fetch_one(&state.db)
        .await?;

    let rows = sqlx::query(
        r#"
        SELECT po.id, po.po_number, po.issue_date, po.currency, po.total_amount, po.status,
               v.legal_name AS vendor_name, c.contract_number,
               (SELECT COUNT(*) FROM purchase_order_line_items li WHERE li.purchase_order_id = po.id) AS line_count
        FROM purchase_orders po
        JOIN vendors v ON v.id = po.vendor_id
        LEFT JOIN contracts c ON c.id = po.contract_id
        ORDER BY po.issue_date DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let pos: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "po_number": r.get::<String, _>("po_number"),
                "issue_date": r.get::<NaiveDate, _>("issue_date"),
                "currency": r.get::<String, _>("currency"),
                "total_amount": r.get::<Decimal, _>("total_amount"),
                "status": r.get::<String, _>("status"),
                "vendor_name": r.get::<String, _>("vendor_name"),
                "contract_number": r.get::<Option<String>, _>("contract_number"),
                "line_count": r.get::<i64, _>("line_count")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({
        "purchase_orders": pos,
        "pagination": pagination_json(total, limit, offset)
    })))
}

#[derive(Deserialize)]
pub struct PoLineReq {
    pub item_code: Option<String>,
    pub description: String,
    pub unit: String,
    pub ordered_quantity: Decimal,
    pub received_quantity: Option<Decimal>,
    pub unit_price: Decimal,
}

#[derive(Deserialize)]
pub struct CreatePoRequest {
    pub po_number: String,
    pub vendor_id: Uuid,
    pub contract_id: Option<Uuid>,
    pub issue_date: NaiveDate,
    pub currency: String,
    pub line_items: Vec<PoLineReq>,
}

pub async fn create_purchase_order(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<CreatePoRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "documents:write")?;
    let total: Decimal = req
        .line_items
        .iter()
        .map(|l| l.ordered_quantity * l.unit_price)
        .sum();
    let mut tx = state.db.begin().await?;
    let po_id: Uuid = sqlx::query_scalar(
        "INSERT INTO purchase_orders (po_number, vendor_id, contract_id, issue_date, currency, total_amount) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(req.po_number.trim())
    .bind(req.vendor_id)
    .bind(req.contract_id)
    .bind(req.issue_date)
    .bind(req.currency.trim().to_ascii_uppercase())
    .bind(total)
    .fetch_one(&mut *tx)
    .await?;

    for l in &req.line_items {
        sqlx::query(
            "INSERT INTO purchase_order_line_items (purchase_order_id, item_code, description, unit, ordered_quantity, received_quantity, unit_price, line_total) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(po_id)
        .bind(&l.item_code)
        .bind(&l.description)
        .bind(&l.unit)
        .bind(l.ordered_quantity)
        .bind(l.received_quantity)
        .bind(l.unit_price)
        .bind(l.ordered_quantity * l.unit_price)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": po_id }))))
}

// =============================================================================
// Invoices & Reconciliation
// =============================================================================
pub async fn list_invoices(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(pg): Query<PaginationQuery>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let (limit, offset) = pg.resolve();
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invoices")
        .fetch_one(&state.db)
        .await?;

    let rows = sqlx::query(
        r#"
        SELECT i.id, i.invoice_number, i.invoice_date, i.currency, i.referenced_contract_number,
               i.referenced_po_number, i.stated_subtotal, i.stated_tax_total, i.stated_total,
               i.recalculated_total, i.match_confidence_status, i.reconciliation_status,
               i.document_id, v.legal_name AS vendor_name,
               (SELECT COUNT(*) FROM anomaly_findings f WHERE f.invoice_id = i.id) AS findings_count
        FROM invoices i
        JOIN vendors v ON v.id = i.vendor_id
        ORDER BY i.created_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let invoices: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "invoice_number": r.get::<String, _>("invoice_number"),
                "invoice_date": r.get::<NaiveDate, _>("invoice_date"),
                "currency": r.get::<String, _>("currency"),
                "referenced_contract_number": r.get::<Option<String>, _>("referenced_contract_number"),
                "referenced_po_number": r.get::<Option<String>, _>("referenced_po_number"),
                "stated_subtotal": r.get::<Decimal, _>("stated_subtotal"),
                "stated_tax_total": r.get::<Decimal, _>("stated_tax_total"),
                "stated_total": r.get::<Decimal, _>("stated_total"),
                "recalculated_total": r.get::<Option<Decimal>, _>("recalculated_total"),
                "match_confidence_status": r.get::<String, _>("match_confidence_status"),
                "reconciliation_status": r.get::<String, _>("reconciliation_status"),
                "document_id": r.get::<Option<Uuid>, _>("document_id"),
                "vendor_name": r.get::<String, _>("vendor_name"),
                "findings_count": r.get::<i64, _>("findings_count")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({
        "invoices": invoices,
        "pagination": pagination_json(total, limit, offset)
    })))
}

#[derive(Deserialize)]
pub struct InvoiceLineReq {
    pub item_code: Option<String>,
    pub description: String,
    pub quantity: Decimal,
    pub unit: String,
    pub unit_price: Decimal,
    pub discount_pct: Option<Decimal>,
    pub discount_amount: Option<Decimal>,
    pub tax_rate: Option<Decimal>,
    pub tax_amount: Option<Decimal>,
    pub stated_line_subtotal: Decimal,
    pub stated_line_total: Decimal,
    pub is_additional_charge: Option<bool>,
}

#[derive(Deserialize)]
pub struct CreateInvoiceRequest {
    pub document_id: Option<Uuid>,
    pub invoice_number: String,
    pub vendor_id: Uuid,
    pub customer_name: Option<String>,
    pub invoice_date: NaiveDate,
    pub currency: String,
    pub referenced_contract_number: Option<String>,
    pub referenced_po_number: Option<String>,
    pub stated_subtotal: Decimal,
    pub stated_discount_total: Option<Decimal>,
    pub stated_tax_total: Option<Decimal>,
    pub stated_additional_charges: Option<Decimal>,
    pub stated_total: Decimal,
    pub line_items: Vec<InvoiceLineReq>,
    pub auto_reconcile: Option<bool>,
}

pub async fn create_invoice(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<CreateInvoiceRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "documents:write")?;
    let mut tx = state.db.begin().await?;

    let inv_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO invoices (
            document_id, invoice_number, vendor_id, customer_name, invoice_date, currency,
            referenced_contract_number, referenced_po_number, stated_subtotal,
            stated_discount_total, stated_tax_total, stated_additional_charges, stated_total
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
        RETURNING id
        "#,
    )
    .bind(req.document_id)
    .bind(req.invoice_number.trim())
    .bind(req.vendor_id)
    .bind(&req.customer_name)
    .bind(req.invoice_date)
    .bind(req.currency.trim().to_ascii_uppercase())
    .bind(&req.referenced_contract_number)
    .bind(&req.referenced_po_number)
    .bind(req.stated_subtotal)
    .bind(req.stated_discount_total.unwrap_or(Decimal::ZERO))
    .bind(req.stated_tax_total.unwrap_or(Decimal::ZERO))
    .bind(req.stated_additional_charges.unwrap_or(Decimal::ZERO))
    .bind(req.stated_total)
    .fetch_one(&mut *tx)
    .await?;

    for (idx, l) in req.line_items.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO invoice_line_items (
                invoice_id, line_number, item_code, description, quantity, unit, unit_price,
                discount_pct, discount_amount, tax_rate, tax_amount,
                stated_line_subtotal, stated_line_total, is_additional_charge
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            "#,
        )
        .bind(inv_id)
        .bind((idx + 1) as i32)
        .bind(&l.item_code)
        .bind(&l.description)
        .bind(l.quantity)
        .bind(&l.unit)
        .bind(l.unit_price)
        .bind(l.discount_pct.unwrap_or(Decimal::ZERO))
        .bind(l.discount_amount.unwrap_or(Decimal::ZERO))
        .bind(l.tax_rate)
        .bind(l.tax_amount.unwrap_or(Decimal::ZERO))
        .bind(l.stated_line_subtotal)
        .bind(l.stated_line_total)
        .bind(l.is_additional_charge.unwrap_or(false))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    if req.auto_reconcile.unwrap_or(true) {
        let (job_id, out) =
            run_invoice_reconciliation_workflow(&state, inv_id, Some(claims.sub), None).await?;
        return Ok((
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": inv_id,
                "reconciliation_job_id": job_id,
                "reconciliation": out
            })),
        ));
    }

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": inv_id }))))
}

pub async fn get_invoice_detail(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let inv_id = parse_uuid(&id, "invoice_id")?;

    let inv = sqlx::query(
        r#"
        SELECT i.*, v.legal_name AS vendor_name, c.contract_number AS matched_contract_number,
               cv.version_label AS matched_contract_version_label
        FROM invoices i
        JOIN vendors v ON v.id = i.vendor_id
        LEFT JOIN contracts c ON c.id = i.matched_contract_id
        LEFT JOIN contract_versions cv ON cv.id = i.matched_contract_version_id
        WHERE i.id = $1
        "#,
    )
    .bind(inv_id)
    .fetch_one(&state.db)
    .await?;

    let line_rows = sqlx::query(
        r#"
        SELECT li.*, ci.unit_price AS contract_unit_price, ci.description AS contract_item_description
        FROM invoice_line_items li
        LEFT JOIN contract_line_items ci ON ci.id = li.matched_contract_item_id
        WHERE li.invoice_id = $1 ORDER BY li.line_number ASC
        "#,
    )
    .bind(inv_id)
    .fetch_all(&state.db)
    .await?;

    let lines: Vec<serde_json::Value> = line_rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "line_number": r.get::<i32, _>("line_number"),
                "item_code": r.get::<Option<String>, _>("item_code"),
                "description": r.get::<String, _>("description"),
                "quantity": r.get::<Decimal, _>("quantity"),
                "unit": r.get::<String, _>("unit"),
                "unit_price": r.get::<Decimal, _>("unit_price"),
                "discount_pct": r.get::<Decimal, _>("discount_pct"),
                "discount_amount": r.get::<Decimal, _>("discount_amount"),
                "tax_rate": r.get::<Option<Decimal>, _>("tax_rate"),
                "tax_amount": r.get::<Decimal, _>("tax_amount"),
                "stated_line_subtotal": r.get::<Decimal, _>("stated_line_subtotal"),
                "stated_line_total": r.get::<Decimal, _>("stated_line_total"),
                "is_additional_charge": r.get::<bool, _>("is_additional_charge"),
                "match_method": r.get::<String, _>("match_method"),
                "match_requires_review": r.get::<bool, _>("match_requires_review"),
                "contract_unit_price": r.get::<Option<Decimal>, _>("contract_unit_price"),
                "contract_item_description": r.get::<Option<String>, _>("contract_item_description")
            })
        })
        .collect();

    let stage_rows = sqlx::query(
        r#"
        SELECT stage_code, stage_name, status, expected_value, actual_value, variance_amount,
               variance_pct, permitted_tolerance, amount_outside_tolerance, currency,
               rule_applied, calculation_details
        FROM reconciliation_results
        WHERE reconciliation_job_id = (
            SELECT id FROM reconciliation_jobs WHERE invoice_id = $1 ORDER BY started_at DESC LIMIT 1
        )
        ORDER BY stage_code ASC, created_at ASC
        "#,
    )
    .bind(inv_id)
    .fetch_all(&state.db)
    .await?;

    let stages: Vec<serde_json::Value> = stage_rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "stage_code": r.get::<String, _>("stage_code"),
                "stage_name": r.get::<String, _>("stage_name"),
                "status": r.get::<String, _>("status"),
                "expected_value": r.get::<Option<Decimal>, _>("expected_value"),
                "actual_value": r.get::<Option<Decimal>, _>("actual_value"),
                "variance_amount": r.get::<Option<Decimal>, _>("variance_amount"),
                "variance_pct": r.get::<Option<Decimal>, _>("variance_pct"),
                "permitted_tolerance": r.get::<Option<String>, _>("permitted_tolerance"),
                "amount_outside_tolerance": r.get::<Option<Decimal>, _>("amount_outside_tolerance"),
                "currency": r.get::<Option<String>, _>("currency"),
                "rule_applied": r.get::<String, _>("rule_applied"),
                "calculation_details": r.get::<serde_json::Value, _>("calculation_details")
            })
        })
        .collect();

    let finding_rows = sqlx::query(
        "SELECT id, category, classification_type, severity, status, reviewer_status, expected_value, actual_value, variance_amount, currency, rule_applied, explanation FROM anomaly_findings WHERE invoice_id = $1 ORDER BY detected_at DESC",
    )
    .bind(inv_id)
    .fetch_all(&state.db)
    .await?;

    let findings: Vec<serde_json::Value> = finding_rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "category": r.get::<String, _>("category"),
                "classification_type": r.get::<String, _>("classification_type"),
                "severity": r.get::<String, _>("severity"),
                "status": r.get::<String, _>("status"),
                "reviewer_status": r.get::<String, _>("reviewer_status"),
                "expected_value": r.get::<Option<Decimal>, _>("expected_value"),
                "actual_value": r.get::<Option<Decimal>, _>("actual_value"),
                "variance_amount": r.get::<Option<Decimal>, _>("variance_amount"),
                "currency": r.get::<Option<String>, _>("currency"),
                "rule_applied": r.get::<String, _>("rule_applied"),
                "explanation": r.get::<String, _>("explanation")
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "invoice": {
            "id": inv.get::<Uuid, _>("id"),
            "document_id": inv.get::<Option<Uuid>, _>("document_id"),
            "invoice_number": inv.get::<String, _>("invoice_number"),
            "vendor_name": inv.get::<String, _>("vendor_name"),
            "invoice_date": inv.get::<NaiveDate, _>("invoice_date"),
            "currency": inv.get::<String, _>("currency"),
            "referenced_contract_number": inv.get::<Option<String>, _>("referenced_contract_number"),
            "matched_contract_number": inv.get::<Option<String>, _>("matched_contract_number"),
            "matched_contract_version_label": inv.get::<Option<String>, _>("matched_contract_version_label"),
            "referenced_po_number": inv.get::<Option<String>, _>("referenced_po_number"),
            "stated_subtotal": inv.get::<Decimal, _>("stated_subtotal"),
            "stated_tax_total": inv.get::<Decimal, _>("stated_tax_total"),
            "stated_additional_charges": inv.get::<Decimal, _>("stated_additional_charges"),
            "stated_total": inv.get::<Decimal, _>("stated_total"),
            "recalculated_subtotal": inv.get::<Option<Decimal>, _>("recalculated_subtotal"),
            "recalculated_tax_total": inv.get::<Option<Decimal>, _>("recalculated_tax_total"),
            "recalculated_total": inv.get::<Option<Decimal>, _>("recalculated_total"),
            "match_confidence_status": inv.get::<String, _>("match_confidence_status"),
            "reconciliation_status": inv.get::<String, _>("reconciliation_status")
        },
        "line_items": lines,
        "stages": stages,
        "findings": findings
    })))
}

pub async fn reconcile_invoice_handler(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "reconciliation:run")?;
    let inv_id = parse_uuid(&id, "invoice_id")?;
    let (job_id, out) =
        run_invoice_reconciliation_workflow(&state, inv_id, Some(claims.sub), None).await?;
    Ok(Json(serde_json::json!({
        "reconciliation_job_id": job_id,
        "result": out
    })))
}

#[derive(Deserialize)]
pub struct ConfirmMatchRequest {
    pub invoice_line_item_id: Uuid,
    pub contract_line_item_id: Uuid,
    pub notes: String,
}

pub async fn confirm_invoice_match(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<ConfirmMatchRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "findings:review")?;
    let inv_id = parse_uuid(&id, "invoice_id")?;

    sqlx::query(
        "UPDATE invoice_line_items SET matched_contract_item_id = $2, match_method = 'human_confirmed', match_requires_review = FALSE WHERE id = $1 AND invoice_id = $3",
    )
    .bind(req.invoice_line_item_id)
    .bind(req.contract_line_item_id)
    .bind(inv_id)
    .execute(&state.db)
    .await?;

    sqlx::query(
        "INSERT INTO review_actions (target_type, target_id, action_type, reviewer_id, new_state, notes) VALUES ('invoice_match', $1, 'confirm_relationship', $2, $3, $4)",
    )
    .bind(req.invoice_line_item_id)
    .bind(claims.sub)
    .bind(serde_json::json!({ "contract_line_item_id": req.contract_line_item_id }))
    .bind(&req.notes)
    .execute(&state.db)
    .await?;

    let (job_id, out) =
        run_invoice_reconciliation_workflow(&state, inv_id, Some(claims.sub), None).await?;
    Ok(Json(serde_json::json!({
        "status": "confirmed_and_reconciled",
        "reconciliation_job_id": job_id,
        "result": out
    })))
}

// =============================================================================
// Payments & Allocations
// =============================================================================
pub async fn list_payments(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(pg): Query<PaginationQuery>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let (limit, offset) = pg.resolve();
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payments")
        .fetch_one(&state.db)
        .await?;

    let rows = sqlx::query(
        r#"
        SELECT p.id, p.payment_reference, p.bank_reference, p.beneficiary_name, p.payment_date,
               p.amount, p.currency, p.referenced_invoice_number, p.transaction_status,
               p.is_refund_or_credit, p.allocation_status, v.legal_name AS vendor_name,
               COALESCE((SELECT SUM(pa.allocated_amount) FROM payment_allocations pa WHERE pa.payment_id = p.id), 0) AS allocated_total
        FROM payments p
        JOIN vendors v ON v.id = p.vendor_id
        ORDER BY p.payment_date DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let payments: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "payment_reference": r.get::<String, _>("payment_reference"),
                "bank_reference": r.get::<Option<String>, _>("bank_reference"),
                "beneficiary_name": r.get::<String, _>("beneficiary_name"),
                "payment_date": r.get::<NaiveDate, _>("payment_date"),
                "amount": r.get::<Decimal, _>("amount"),
                "currency": r.get::<String, _>("currency"),
                "referenced_invoice_number": r.get::<Option<String>, _>("referenced_invoice_number"),
                "transaction_status": r.get::<String, _>("transaction_status"),
                "is_refund_or_credit": r.get::<bool, _>("is_refund_or_credit"),
                "allocation_status": r.get::<String, _>("allocation_status"),
                "vendor_name": r.get::<String, _>("vendor_name"),
                "allocated_total": r.get::<Decimal, _>("allocated_total")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({
        "payments": payments,
        "pagination": pagination_json(total, limit, offset)
    })))
}

#[derive(Deserialize)]
pub struct CreatePaymentRequest {
    pub payment_reference: String,
    pub bank_reference: Option<String>,
    pub vendor_id: Uuid,
    pub beneficiary_name: String,
    pub payment_date: NaiveDate,
    pub amount: Decimal,
    pub currency: String,
    pub referenced_invoice_number: Option<String>,
    pub is_refund_or_credit: Option<bool>,
}

pub async fn create_payment(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<CreatePaymentRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "payments:write")?;
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO payments (payment_reference, bank_reference, vendor_id, beneficiary_name, payment_date, amount, currency, referenced_invoice_number, is_refund_or_credit)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id
        "#,
    )
    .bind(req.payment_reference.trim())
    .bind(&req.bank_reference)
    .bind(req.vendor_id)
    .bind(req.beneficiary_name.trim())
    .bind(req.payment_date)
    .bind(req.amount)
    .bind(req.currency.trim().to_ascii_uppercase())
    .bind(&req.referenced_invoice_number)
    .bind(req.is_refund_or_credit.unwrap_or(false))
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

#[derive(Deserialize)]
pub struct AllocatePaymentRequest {
    pub invoice_id: Uuid,
    pub allocated_amount: Decimal,
    pub matching_evidence: String,
}

pub async fn allocate_payment(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<AllocatePaymentRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "findings:review")?;
    let pay_id = parse_uuid(&id, "payment_id")?;

    let pay = sqlx::query("SELECT currency, amount FROM payments WHERE id = $1")
        .bind(pay_id)
        .fetch_one(&state.db)
        .await?;
    let p_curr: String = pay.get("currency");
    let p_amt: Decimal = pay.get("amount");

    let inv_curr: String = sqlx::query_scalar("SELECT currency FROM invoices WHERE id = $1")
        .bind(req.invoice_id)
        .fetch_one(&state.db)
        .await?;

    if !p_curr.eq_ignore_ascii_case(&inv_curr) {
        return Err(AppError::BadRequest(format!(
            "Cannot allocate {} payment to {} invoice without verified FX conversion.",
            p_curr, inv_curr
        )));
    }

    sqlx::query(
        r#"
        INSERT INTO payment_allocations (payment_id, invoice_id, allocated_amount, currency, matching_evidence, matched_by, confirmed_by)
        VALUES ($1,$2,$3,$4,$5,'human_confirmed',$6)
        ON CONFLICT (payment_id, invoice_id) DO UPDATE SET allocated_amount = EXCLUDED.allocated_amount, matching_evidence = EXCLUDED.matching_evidence
        "#,
    )
    .bind(pay_id)
    .bind(req.invoice_id)
    .bind(req.allocated_amount)
    .bind(&p_curr)
    .bind(&req.matching_evidence)
    .bind(claims.sub)
    .execute(&state.db)
    .await?;

    let total_alloc: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(allocated_amount), 0) FROM payment_allocations WHERE payment_id = $1",
    )
    .bind(pay_id)
    .fetch_one(&state.db)
    .await?;

    let new_status = if total_alloc == p_amt {
        "fully_allocated"
    } else if total_alloc > p_amt {
        "over_allocated"
    } else {
        "partially_allocated"
    };

    sqlx::query("UPDATE payments SET allocation_status = $2 WHERE id = $1")
        .bind(pay_id)
        .bind(new_status)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({ "status": new_status, "allocated_total": total_alloc })))
}
