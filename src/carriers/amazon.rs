// Amazon Logistics (TBA…) provider. Amazon has no buyer-facing tracking
// API; track.amazon.com is the recipient-facing tracker and serves its
// data from an unauthenticated JSON endpoint, so this provider needs no
// credentials at all. Sub-documents (progressTracker, eventHistory)
// arrive as JSON-encoded *strings* inside the outer JSON — unwrap() them
// via `nested()` before reading.

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use super::{join_location, parse_carrier_datetime};
use crate::models::{TrackingEvent, TrackingInfo};

pub const API_BASE: &str = "https://track.amazon.com";

pub struct Amazon;

impl Amazon {
    pub async fn track(
        &self,
        http: &reqwest::Client,
        tracking_number: &str,
    ) -> Result<TrackingInfo> {
        let resp = http
            .get(format!("{}/api/tracker/{}", API_BASE, tracking_number))
            .header("Accept", "application/json")
            .header(
                "User-Agent",
                "Mozilla/5.0 (X11; Linux x86_64) parceltracker",
            )
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("Amazon tracker returned {}", status));
        }
        let json: Value = resp.json().await.context("Amazon tracker: invalid JSON")?;
        parse_response(&json)
    }
}

/// Fetch a sub-document that may be inline JSON or a JSON-encoded string.
fn nested(json: &Value, key: &str) -> Option<Value> {
    match json.get(key)? {
        Value::String(s) => serde_json::from_str(s).ok(),
        Value::Null => None,
        other => Some(other.clone()),
    }
}

/// "OUT_FOR_DELIVERY" → "Out for delivery"
fn humanize(code: &str) -> String {
    let s = code.replace('_', " ").to_lowercase();
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => s,
    }
}

/// Map Amazon status / event codes onto the shared status vocabulary.
fn map_status(code: &str) -> String {
    let compact = code.to_uppercase();
    if compact.contains("DELIVERED") && !compact.contains("UNDELIVER") {
        return "Delivered".to_string();
    }
    if compact.contains("OUT_FOR_DELIVERY") {
        return "OutForDelivery".to_string();
    }
    match compact.as_str() {
        "CREATED" | "MANIFESTED" | "ORDER_RECEIVED" | "SHIPPING_LABEL_CREATED" => {
            "InfoReceived".to_string()
        }
        "UNDELIVERABLE" | "DELIVERY_ATTEMPTED" | "REJECTED" | "LOST" | "DAMAGED" | "RETURNED"
        | "RETURNING" => "Exception".to_string(),
        "" => "InTransit".to_string(),
        _ => "InTransit".to_string(),
    }
}

pub fn parse_response(json: &Value) -> Result<TrackingInfo> {
    let progress = nested(json, "progressTracker")
        .context("Amazon tracker: no progressTracker in response")?;

    if let Some(err) = progress
        .get("errors")
        .and_then(|e| e.as_array())
        .and_then(|a| a.first())
    {
        return Err(anyhow!(
            "Amazon: {}",
            err.get("errorMessage")
                .or_else(|| err.get("errorCode"))
                .and_then(|m| m.as_str())
                .unwrap_or("tracking error")
        ));
    }

    let summary_status = progress
        .pointer("/summary/status")
        .and_then(|s| s.as_str())
        .unwrap_or("");

    let eta = progress
        .get("expectedDeliveryDate")
        .and_then(|d| d.as_str())
        .and_then(parse_carrier_datetime);

    let mut events: Vec<TrackingEvent> = nested(json, "eventHistory")
        .and_then(|h| h.get("eventHistory").cloned())
        .and_then(|e| e.as_array().cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|ev| {
                    let date = ev
                        .get("eventTime")
                        .and_then(|t| t.as_str())
                        .and_then(parse_carrier_datetime)?;
                    let code = ev.get("eventCode").and_then(|c| c.as_str()).unwrap_or("");
                    let loc = ev.get("location");
                    Some(TrackingEvent {
                        date,
                        description: humanize(code),
                        location: join_location(&[
                            loc.and_then(|l| l.get("city")).and_then(|v| v.as_str()),
                            loc.and_then(|l| l.get("stateProvince")).and_then(|v| v.as_str()),
                            loc.and_then(|l| l.get("countryCode")).and_then(|v| v.as_str()),
                        ]),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    events.sort_by(|a, b| b.date.cmp(&a.date));

    let current_location = events
        .first()
        .map(|e| e.location.clone())
        .filter(|l| !l.is_empty());

    // Status: the summary when present, else the newest event's code.
    let effective = if summary_status.is_empty() {
        // events are humanized already; re-derive from raw code is lost, so
        // classify the humanized text (the sniffing is case-insensitive).
        events
            .first()
            .map(|e| e.description.clone())
            .unwrap_or_default()
    } else {
        summary_status.to_string()
    };

    Ok(TrackingInfo {
        status_code: map_status(&effective.replace(' ', "_")),
        status_text: Some(if effective.is_empty() {
            "In transit".to_string()
        } else {
            humanize(&effective)
        }),
        current_location,
        estimated_delivery_date: eta,
        events,
        raw_data: Some(json.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // progressTracker and eventHistory are JSON-encoded strings, exactly
    // as the live endpoint returns them.
    fn fixture() -> Value {
        let progress = serde_json::json!({
            "errors": null,
            "summary": { "status": "OUT_FOR_DELIVERY" },
            "expectedDeliveryDate": "2026-08-15T21:00:00Z",
            "trackerSource": "AMZL"
        });
        let history = serde_json::json!({
            "eventHistory": [
                {
                    "eventCode": "CREATED",
                    "eventTime": "2026-08-13T09:00:00Z",
                    "location": null
                },
                {
                    "eventCode": "ARRIVED_AT_DELIVERY_Center",
                    "eventTime": "2026-08-14T04:00:00Z",
                    "location": { "city": "Kent", "stateProvince": "WA", "countryCode": "US" }
                },
                {
                    "eventCode": "OUT_FOR_DELIVERY",
                    "eventTime": "2026-08-15T13:30:00Z",
                    "location": { "city": "Seattle", "stateProvince": "WA", "countryCode": "US" }
                }
            ]
        });
        serde_json::json!({
            "progressTracker": progress.to_string(),
            "eventHistory": history.to_string()
        })
    }

    #[test]
    fn parses_stringified_subdocuments() {
        let info = parse_response(&fixture()).unwrap();
        assert_eq!(info.status_code, "OutForDelivery");
        assert_eq!(info.status_text.as_deref(), Some("Out for delivery"));
        assert_eq!(info.current_location.as_deref(), Some("Seattle, WA, US"));
        assert!(info
            .estimated_delivery_date
            .unwrap()
            .to_rfc3339()
            .starts_with("2026-08-15"));
        // newest-first
        assert_eq!(info.events.len(), 3);
        assert_eq!(info.events[0].description, "Out for delivery");
        assert_eq!(info.events[2].description, "Created");
    }

    #[test]
    fn unknown_tracking_id_is_an_error() {
        // Shape captured from the live endpoint for a bogus TBA number.
        let progress = serde_json::json!({
            "errors": [ { "errorCode": "TRACKING_ID_NOT_FOUND", "errorMessage": "INVALID TRACKING_ID" } ],
            "summary": { "status": null },
            "expectedDeliveryDate": null
        });
        let json = serde_json::json!({ "progressTracker": progress.to_string(), "eventHistory": null });
        let err = parse_response(&json).unwrap_err();
        assert!(err.to_string().contains("INVALID TRACKING_ID"));
    }

    #[test]
    fn delivered_summary_maps() {
        let progress = serde_json::json!({
            "summary": { "status": "DELIVERED" },
            "expectedDeliveryDate": null
        });
        let json = serde_json::json!({ "progressTracker": progress.to_string(), "eventHistory": null });
        let info = parse_response(&json).unwrap();
        assert_eq!(info.status_code, "Delivered");
        assert!(info.events.is_empty());
    }
}
