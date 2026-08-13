use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::api::get_tracking_url;
use crate::models::Parcel;
use crate::waybar::{format_eta_smart, resolve_waybar_parcel};

/// Build the `status --json` document. Pure function of its inputs so tests
/// can pin the shape; `selected` is the tracking number the bar leads with.
///
/// The document is a stable contract consumed by the Omarchy shell plugin
/// (omarchy-plugin/Model.js): bump `version` on breaking changes.
pub fn build_json(parcels: &[Parcel], selected: Option<&str>, now: DateTime<Utc>) -> Value {
    let items: Vec<Value> = parcels
        .iter()
        .map(|parcel| {
            let info = parcel.tracking_info.as_ref();
            let carrier = parcel.resolved_carrier();

            let mut events: Vec<&crate::models::TrackingEvent> =
                info.map(|i| i.events.iter().collect()).unwrap_or_default();
            events.sort_by(|a, b| b.date.cmp(&a.date));

            let eta = info.and_then(|i| i.estimated_delivery_date.as_ref());

            json!({
                "id": parcel.id,
                "trackingNumber": parcel.tracking_number,
                "carrier": carrier.name(),
                "description": parcel.description,
                "state": parcel.status_state(),
                "statusText": info.map(|i| i.status_text()),
                "location": info.and_then(|i| i.current_location.clone()),
                "eta": eta.map(|d| d.to_rfc3339()),
                "etaLabel": eta.map(|d| format_eta_smart(&d.to_rfc3339())),
                "daysUntilDelivery": parcel.days_until_delivery(),
                "lastUpdated": parcel.last_updated.map(|d| d.to_rfc3339()),
                "trackingUrl": get_tracking_url(&carrier, &parcel.tracking_number),
                "events": events.iter().map(|e| json!({
                    "time": e.date.to_rfc3339(),
                    "description": e.description,
                    "location": e.location,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    json!({
        "version": 1,
        "generatedAt": now.to_rfc3339(),
        "selected": selected,
        "parcels": items,
    })
}

pub fn get_json_output(parcels: &[Parcel]) -> Result<String> {
    let selected = resolve_waybar_parcel(parcels)?.map(|p| p.tracking_number.clone());
    let doc = build_json(parcels, selected.as_deref(), Utc::now());
    Ok(serde_json::to_string_pretty(&doc)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TrackingEvent, TrackingInfo};
    use chrono::TimeZone;

    fn parcel(tracking: &str, desc: &str, status: &str, eta: Option<DateTime<Utc>>) -> Parcel {
        let mut p = Parcel::new(tracking.to_string(), desc.to_string(), "auto".to_string());
        p.tracking_info = Some(TrackingInfo {
            status_code: status.to_string(),
            status_text: Some(status.to_string()),
            current_location: Some("MEMPHIS, TN".to_string()),
            estimated_delivery_date: eta,
            events: vec![
                TrackingEvent {
                    date: Utc.with_ymd_and_hms(2026, 8, 11, 9, 0, 0).unwrap(),
                    description: "Departed facility".to_string(),
                    location: "Louisville, KY".to_string(),
                },
                TrackingEvent {
                    date: Utc.with_ymd_and_hms(2026, 8, 12, 14, 30, 0).unwrap(),
                    description: "Arrived at hub".to_string(),
                    location: "Memphis, TN".to_string(),
                },
            ],
            raw_data: None,
        });
        p
    }

    #[test]
    fn document_shape_and_states() {
        let eta = Utc.with_ymd_and_hms(2026, 8, 15, 20, 0, 0).unwrap();
        let parcels = vec![
            parcel("1Z999AA10123456784", "Keyboard", "In transit", Some(eta)),
            parcel("9400111899223100000000", "Socks", "Delivered", None),
            parcel(
                "420912349205590000000000000000",
                "Lamp",
                "Out for delivery",
                None,
            ),
            parcel(
                "JD014600003RUSSIA",
                "Widget",
                "Undelivered - address issue",
                None,
            ),
        ];

        let doc = build_json(
            &parcels,
            Some("1Z999AA10123456784"),
            Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap(),
        );

        assert_eq!(doc["version"], 1);
        assert_eq!(doc["selected"], "1Z999AA10123456784");
        let items = doc["parcels"].as_array().unwrap();
        assert_eq!(items.len(), 4);

        assert_eq!(items[0]["state"], "in-transit");
        assert_eq!(items[0]["carrier"], "UPS");
        assert!(items[0]["trackingUrl"]
            .as_str()
            .unwrap()
            .contains("ups.com"));
        assert!(items[0]["eta"].as_str().unwrap().starts_with("2026-08-15"));
        assert!(items[0]["etaLabel"].is_string());

        assert_eq!(items[1]["state"], "delivered");
        assert_eq!(items[2]["state"], "out-for-delivery");
        // "Undelivered" must not read as delivered.
        assert_eq!(items[3]["state"], "exception");
    }

    #[test]
    fn events_sorted_newest_first() {
        let parcels = vec![parcel("1Z999AA10123456784", "Keyboard", "In transit", None)];
        let doc = build_json(&parcels, None, Utc::now());
        let events = doc["parcels"][0]["events"].as_array().unwrap();
        assert_eq!(events.len(), 2);
        assert!(events[0]["time"].as_str().unwrap() > events[1]["time"].as_str().unwrap());
        assert_eq!(events[0]["description"], "Arrived at hub");
    }

    #[test]
    fn untracked_parcel_serializes_with_nulls() {
        let p = Parcel::new("XYZ".to_string(), "Mystery".to_string(), "auto".to_string());
        let doc = build_json(&[p], None, Utc::now());
        let item = &doc["parcels"][0];
        assert_eq!(item["state"], "unknown");
        assert!(item["eta"].is_null());
        assert!(item["statusText"].is_null());
        assert_eq!(item["events"].as_array().unwrap().len(), 0);
    }
}
