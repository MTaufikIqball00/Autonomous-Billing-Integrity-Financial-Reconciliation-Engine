use chrono::NaiveDate;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// ISO 4217 minor-unit decimal places (never assumes all currencies have 2 decimals)
pub fn currency_decimal_places(currency: &str) -> u32 {
    match currency.trim().to_ascii_uppercase().as_str() {
        "JPY" | "KRW" | "VND" | "CLP" | "PYG" | "UGX" | "RWF" => 0,
        "BHD" | "KWD" | "OMR" | "JOD" | "IQD" | "LYD" | "TND" => 3,
        _ => 2,
    }
}

pub fn round_for_currency(amount: Decimal, currency: &str) -> Decimal {
    let scale = currency_decimal_places(currency);
    amount.round_dp_with_strategy(scale, RoundingStrategy::MidpointAwayFromZero)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum NormalizationOutcome<T> {
    Normalized {
        value: T,
        original: String,
        rule: String,
    },
    Ambiguous {
        original: String,
        reason: String,
    },
    Invalid {
        original: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NormalizedMoney {
    pub amount: Decimal,
    pub currency: Option<String>,
    pub decimal_places: u32,
}

/// Deterministically parses monetary values without silently coercing ambiguous separators.
pub fn normalize_money(raw: &str, default_currency: Option<&str>) -> NormalizationOutcome<NormalizedMoney> {
    let original = raw.trim().to_string();
    if original.is_empty() {
        return NormalizationOutcome::Invalid {
            original,
            reason: "Empty monetary string".into(),
        };
    }

    let mut working = original.clone();
    let mut is_negative = false;

    // Accounting parentheses e.g. "(1,250.00)"
    if working.starts_with('(') && working.ends_with(')') {
        is_negative = true;
        working = working[1..working.len() - 1].trim().to_string();
    }
    if working.starts_with('-') {
        is_negative = true;
        working = working[1..].trim().to_string();
    } else if working.starts_with('+') {
        working = working[1..].trim().to_string();
    }

    // Detect explicit currency code or symbol
    let mut detected_currency: Option<String> = None;
    for code in ["USD", "EUR", "GBP", "JPY", "KWD", "BHD", "OMR", "IDR", "SGD", "AUD", "CAD", "CHF"] {
        if working.to_ascii_uppercase().starts_with(code) {
            detected_currency = Some(code.to_string());
            working = working[code.len()..].trim().to_string();
            break;
        } else if working.to_ascii_uppercase().ends_with(code) {
            detected_currency = Some(code.to_string());
            working = working[..working.len() - code.len()].trim().to_string();
            break;
        }
    }

    if let Some(rest) = working.strip_prefix('$') {
        // "$" alone can be USD/CAD/AUD/SGD; use default_currency if provided, else keep None (ambiguous symbol)
        working = rest.trim().to_string();
        if detected_currency.is_none() {
            detected_currency = default_currency.map(|c| c.to_ascii_uppercase());
        }
    } else if let Some(rest) = working.strip_prefix('€') {
        working = rest.trim().to_string();
        detected_currency.get_or_insert_with(|| "EUR".into());
    } else if let Some(rest) = working.strip_prefix('£') {
        working = rest.trim().to_string();
        detected_currency.get_or_insert_with(|| "GBP".into());
    } else if let Some(rest) = working.strip_prefix('¥') {
        working = rest.trim().to_string();
        detected_currency.get_or_insert_with(|| "JPY".into());
    }

    if detected_currency.is_none() {
        detected_currency = default_currency.map(|c| c.to_ascii_uppercase());
    }

    let cleaned = working.replace([' ', '\u{00a0}'], "");
    if cleaned.is_empty() || !cleaned.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '.') {
        return NormalizationOutcome::Invalid {
            original,
            reason: "Contains non-numeric characters after currency stripping".into(),
        };
    }

    let has_comma = cleaned.contains(',');
    let has_dot = cleaned.contains('.');

    let (canonical_num, rule) = match (has_comma, has_dot) {
        (false, false) => (cleaned.clone(), "plain_integer".to_string()),
        (true, true) => {
            let last_comma = cleaned.rfind(',').unwrap();
            let last_dot = cleaned.rfind('.').unwrap();
            if last_dot > last_comma {
                // US/UK format: 1,234.56
                (cleaned.replace(',', ""), "us_thousands_comma_decimal_dot".to_string())
            } else {
                // European format: 1.234,56
                (
                    cleaned.replace('.', "").replace(',', "."),
                    "eu_thousands_dot_decimal_comma".to_string(),
                )
            }
        }
        (false, true) => {
            let parts: Vec<&str> = cleaned.split('.').collect();
            if parts.len() > 2 {
                // Multiple dots -> European thousands separators e.g. 1.234.567
                if parts[1..].iter().all(|p| p.len() == 3) {
                    (cleaned.replace('.', ""), "eu_multi_thousands_dot".to_string())
                } else {
                    return NormalizationOutcome::Invalid {
                        original,
                        reason: "Malformed multiple decimal points".into(),
                    };
                }
            } else {
                // Single dot: 123.45 or 1.234 (Note: in KWD/BHD 3 decimals is valid; in USD/EUR single dot with 3 digits could be ambiguous if EU)
                let frac_len = parts[1].len();
                let is_3dp_currency = matches!(
                    detected_currency.as_deref(),
                    Some("KWD" | "BHD" | "OMR" | "JOD" | "IQD" | "LYD" | "TND")
                );
                if frac_len == 3 && !is_3dp_currency && parts[0].len() <= 3 && parts[0] != "0" {
                    return NormalizationOutcome::Ambiguous {
                        original,
                        reason: "Single dot with 3 trailing digits may be EU thousands separator or 3-decimal value; requires review.".into(),
                    };
                }
                (cleaned.clone(), "standard_decimal_dot".to_string())
            }
        }
        (true, false) => {
            let parts: Vec<&str> = cleaned.split(',').collect();
            if parts.len() > 2 {
                // Multiple commas -> US thousands e.g. 1,234,567
                if parts[1..].iter().all(|p| p.len() == 3) {
                    (cleaned.replace(',', ""), "us_multi_thousands_comma".to_string())
                } else {
                    return NormalizationOutcome::Invalid {
                        original,
                        reason: "Malformed multiple commas".into(),
                    };
                }
            } else {
                let frac_len = parts[1].len();
                if frac_len == 2 || frac_len == 1 {
                    (cleaned.replace(',', "."), "eu_decimal_comma".to_string())
                } else if frac_len == 3 {
                    // Could be 1,000 (US thousands) OR 1,250 (EU 3-decimal)
                    return NormalizationOutcome::Ambiguous {
                        original,
                        reason: "Single comma with 3 trailing digits is ambiguous between thousands separator and decimal comma; requires confirmation.".into(),
                    };
                } else {
                    return NormalizationOutcome::Invalid {
                        original,
                        reason: "Unrecognized comma-separated numeric format".into(),
                    };
                }
            }
        }
    };

    match Decimal::from_str(&canonical_num) {
        Ok(mut dec) => {
            if is_negative {
                dec = -dec;
            }
            let dp = detected_currency
                .as_deref()
                .map(currency_decimal_places)
                .unwrap_or(2);
            NormalizationOutcome::Normalized {
                value: NormalizedMoney {
                    amount: dec,
                    currency: detected_currency,
                    decimal_places: dp,
                },
                original,
                rule,
            }
        }
        Err(_) => NormalizationOutcome::Invalid {
            original,
            reason: "Failed to parse canonical decimal value".into(),
        },
    }
}

/// Parses percentages like "10%", "10.5%", or "0.10" (returns percentage points e.g. 10.00 for 10%)
pub fn normalize_percentage(raw: &str) -> NormalizationOutcome<Decimal> {
    let original = raw.trim().to_string();
    let stripped = original.trim_end_matches('%').trim();
    match Decimal::from_str(stripped) {
        Ok(v) if v >= Decimal::ZERO && v <= Decimal::from(100) => NormalizationOutcome::Normalized {
            value: v,
            original,
            rule: "percentage_points_0_to_100".into(),
        },
        Ok(_) => NormalizationOutcome::Invalid {
            original,
            reason: "Percentage out of valid 0..=100 range".into(),
        },
        Err(_) => NormalizationOutcome::Invalid {
            original,
            reason: "Unable to parse percentage".into(),
        },
    }
}

/// Parses dates deterministically without guessing ambiguous MM/DD/YYYY vs DD/MM/YYYY
pub fn normalize_date(raw: &str) -> NormalizationOutcome<NaiveDate> {
    let original = raw.trim().to_string();
    if let Ok(d) = NaiveDate::parse_from_str(&original, "%Y-%m-%d") {
        return NormalizationOutcome::Normalized {
            value: d,
            original,
            rule: "iso_8601_yyyy_mm_dd".into(),
        };
    }
    if let Ok(d) = NaiveDate::parse_from_str(&original, "%Y/%m/%d") {
        return NormalizationOutcome::Normalized {
            value: d,
            original,
            rule: "yyyy_slash_mm_slash_dd".into(),
        };
    }
    for (fmt, rule) in [
        ("%B %d, %Y", "long_month_d_yyyy"),
        ("%b %d, %Y", "short_month_d_yyyy"),
        ("%d %B %Y", "d_long_month_yyyy"),
        ("%d %b %Y", "d_short_month_yyyy"),
    ] {
        if let Ok(d) = NaiveDate::parse_from_str(&original, fmt) {
            return NormalizationOutcome::Normalized {
                value: d,
                original,
                rule: rule.into(),
            };
        }
    }

    // Check slash/dash dates where ambiguity may exist
    let sep = if original.contains('/') {
        Some('/')
    } else if original.contains('-') {
        Some('-')
    } else {
        None
    };

    if let Some(ch) = sep {
        let parts: Vec<&str> = original.split(ch).collect();
        if parts.len() == 3 && parts[2].len() == 4 {
            if let (Ok(a), Ok(b), Ok(y)) = (
                parts[0].parse::<u32>(),
                parts[1].parse::<u32>(),
                parts[2].parse::<i32>(),
            ) {
                if a <= 12 && b <= 12 && a != b {
                    return NormalizationOutcome::Ambiguous {
                        original,
                        reason: "Ambiguous numeric date (both day and month <= 12); cannot determine MM/DD vs DD/MM silently.".into(),
                    };
                }
                if a > 12 && b <= 12 {
                    if let Some(d) = NaiveDate::from_ymd_opt(y, b, a) {
                        return NormalizationOutcome::Normalized {
                            value: d,
                            original,
                            rule: "unambiguous_dd_mm_yyyy".into(),
                        };
                    }
                }
                if b > 12 && a <= 12 {
                    if let Some(d) = NaiveDate::from_ymd_opt(y, a, b) {
                        return NormalizationOutcome::Normalized {
                            value: d,
                            original,
                            rule: "unambiguous_mm_dd_yyyy".into(),
                        };
                    }
                }
                if a == b && a <= 12 {
                    if let Some(d) = NaiveDate::from_ymd_opt(y, a, b) {
                        return NormalizationOutcome::Normalized {
                            value: d,
                            original,
                            rule: "symmetric_day_month_yyyy".into(),
                        };
                    }
                }
            }
        }
    }

    NormalizationOutcome::Invalid {
        original,
        reason: "Unrecognized date format".into(),
    }
}

pub fn normalize_unit(raw: &str) -> (String, &'static str) {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "ea" | "each" | "pc" | "pcs" | "piece" | "pieces" | "unit" | "units" => ("unit".into(), "unit_canonical_each"),
        "hr" | "hrs" | "hour" | "hours" => ("hour".into(), "unit_canonical_hour"),
        "day" | "days" => ("day".into(), "unit_canonical_day"),
        "mo" | "month" | "months" | "mth" => ("month".into(), "unit_canonical_month"),
        "yr" | "year" | "years" => ("year".into(), "unit_canonical_year"),
        "seat" | "seats" | "license" | "licenses" | "user" | "users" => ("seat".into(), "unit_canonical_seat"),
        "kg" | "kgs" | "kilogram" | "kilograms" => ("kg".into(), "unit_canonical_kg"),
        "gb" | "gigabyte" | "gigabytes" => ("gb".into(), "unit_canonical_gb"),
        other => (other.to_string(), "unit_passthrough_lowercase"),
    }
}
