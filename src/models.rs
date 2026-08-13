use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// Config and Waybar types
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(rename = "track17_api_key", skip_serializing_if = "Option::is_none")]
    pub track17_api_key: Option<String>,
    #[serde(rename = "waybar_selected", skip_serializing_if = "Option::is_none")]
    pub waybar_selected: Option<WaybarSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaybarSelection {
    pub tracking: String,
    #[serde(
        deserialize_with = "deserialize_timestamp",
        serialize_with = "serialize_timestamp"
    )]
    pub timestamp: String,
}

fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    // Try to deserialize as a Value first to inspect the type
    let value = serde_json::Value::deserialize(deserializer)?;

    // Try string first (new format)
    if let Some(s) = value.as_str() {
        return Ok(s.to_string());
    }

    // Try integer (old Python format - Unix timestamp)
    if let Some(ts) = value.as_u64() {
        let datetime = chrono::DateTime::from_timestamp(ts as i64, 0)
            .ok_or_else(|| D::Error::custom("Invalid timestamp"))?;
        return Ok(datetime.to_rfc3339());
    }

    Err(D::Error::custom("Expected string or integer timestamp"))
}

fn serialize_timestamp<S>(timestamp: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(timestamp)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cache {
    pub parcels: Vec<Parcel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_cache_time: Option<DateTime<Utc>>,
}

// Carrier enum
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Carrier {
    UPS,
    FedEx,
    DHL,
    USPS,
    CanadaPost,
    OnTrac,
    Unknown,
}

impl Carrier {
    pub fn name(&self) -> &'static str {
        match self {
            Carrier::UPS => "UPS",
            Carrier::FedEx => "FedEx",
            Carrier::DHL => "DHL",
            Carrier::USPS => "USPS",
            Carrier::CanadaPost => "Canada Post",
            Carrier::OnTrac => "OnTrac",
            Carrier::Unknown => "Unknown",
        }
    }

    pub fn detect(tracking: &str) -> Self {
        use regex::Regex;

        let tracking = tracking.to_uppercase().replace(" ", "").replace("-", "");

        // UPS: 1Z followed by alphanumeric
        if Regex::new(r"^1Z[A-Z0-9]{16}$").unwrap().is_match(&tracking) {
            return Carrier::UPS;
        }

        // FedEx: 12 digits or 15 digits or 20/22/34 digits
        if Regex::new(r"^\d{12}$").unwrap().is_match(&tracking)
            || Regex::new(r"^\d{15}$").unwrap().is_match(&tracking)
            || Regex::new(r"^\d{20}$").unwrap().is_match(&tracking)
            || Regex::new(r"^\d{22}$").unwrap().is_match(&tracking)
            || Regex::new(r"^\d{34}$").unwrap().is_match(&tracking)
        {
            return Carrier::FedEx;
        }

        // DHL: 10 digits, or starts with JJ/JD/JM
        if Regex::new(r"^\d{10}$").unwrap().is_match(&tracking)
            || Regex::new(r"^(JJ|JD|JM)\d{15,}")
                .unwrap()
                .is_match(&tracking)
        {
            return Carrier::DHL;
        }

        // USPS: 20-22 digits, or 13 chars starting with letters, or specific formats
        if Regex::new(r"^\d{20,22}$").unwrap().is_match(&tracking)
            || Regex::new(r"^[A-Z]{2}\d{9}[A-Z]{2}$")
                .unwrap()
                .is_match(&tracking)
            || Regex::new(r"^(94|93|92|95|96)\d{20}$")
                .unwrap()
                .is_match(&tracking)
        {
            return Carrier::USPS;
        }

        // Canada Post: 16 digits or 13 chars (pattern similar to USPS but with different prefix)
        if Regex::new(r"^\d{16}$").unwrap().is_match(&tracking)
            || (Regex::new(r"^[A-Z]{2}\d{9}CA$")
                .unwrap()
                .is_match(&tracking))
        {
            return Carrier::CanadaPost;
        }

        // OnTrac: 15 chars starting with C or D, or 7-8 digits
        if Regex::new(r"^[CD]\d{14}$").unwrap().is_match(&tracking)
            || Regex::new(r"^\d{7,8}$").unwrap().is_match(&tracking)
        {
            return Carrier::OnTrac;
        }

        Carrier::Unknown
    }
}

// Tracking types
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackingInfo {
    pub status_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_delivery_date: Option<DateTime<Utc>>,
    pub events: Vec<TrackingEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_data: Option<serde_json::Value>,
}

impl TrackingInfo {
    pub fn status_text(&self) -> String {
        self.status_text.clone().unwrap_or_else(|| {
            if self.status_code.is_empty() {
                "Unknown".to_string()
            } else {
                self.status_code.clone()
            }
        })
    }

    pub fn is_delivered(&self) -> bool {
        self.status_code.to_lowercase().contains("delivered")
            || self
                .status_text
                .as_ref()
                .map(|s| s.to_lowercase().contains("delivered"))
                .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingEvent {
    pub date: DateTime<Utc>,
    pub description: String,
    pub location: String,
}

// Parcel struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parcel {
    pub id: String,
    pub tracking_number: String,
    pub carrier: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracking_info: Option<TrackingInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<DateTime<Utc>>,
}

impl Parcel {
    pub fn new(tracking_number: String, description: String, carrier: String) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let id = format!(
            "{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );
        Self {
            id,
            tracking_number,
            description,
            carrier,
            tracking_info: None,
            last_updated: None,
        }
    }

    pub fn display_name(&self) -> String {
        if self.description.is_empty() {
            self.tracking_number.clone()
        } else {
            format!("{} ({})", self.description, self.tracking_number)
        }
    }

    pub fn is_delivered(&self) -> bool {
        self.tracking_info
            .as_ref()
            .map(|info| info.is_delivered())
            .unwrap_or(false)
    }

    pub fn status_summary(&self) -> String {
        self.tracking_info
            .as_ref()
            .map(|info| info.status_text())
            .unwrap_or_else(|| "Not tracked".to_string())
    }

    /// Canonical machine-readable state, derived from the carrier status the
    /// same way status_emoji always has. The string set is a stable contract
    /// consumed by the Omarchy shell plugin (omarchy-plugin/Model.js).
    pub fn status_state(&self) -> &'static str {
        let status = self
            .tracking_info
            .as_ref()
            .map(|t| format!("{} {}", t.status_code, t.status_text()))
            .unwrap_or_else(|| "unknown".to_string())
            .to_lowercase();

        let status_compact: String = status
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();

        if status.contains("delivered") && !status.contains("undelivered") {
            "delivered"
        } else if status.contains("out for delivery")
            || status.contains("out_for_delivery")
            || status_compact.contains("outfordelivery")
        {
            "out-for-delivery"
        } else if status.contains("in transit")
            || status.contains("in_transit")
            || status_compact.contains("intransit")
        {
            "in-transit"
        } else if status.contains("info") || status.contains("pre") {
            "pre-transit"
        } else if status.contains("exception")
            || status.contains("fail")
            || status.contains("undelivered")
        {
            "exception"
        } else {
            "unknown"
        }
    }

    pub fn status_emoji(&self) -> &'static str {
        match self.status_state() {
            "delivered" => "✅",
            "out-for-delivery" => "🚚",
            "in-transit" => "📦",
            "pre-transit" => "📝",
            "exception" => "⚠️",
            _ => "📍",
        }
    }

    /// The carrier as an enum, honoring an explicit setting and falling back
    /// to detection from the tracking number.
    pub fn resolved_carrier(&self) -> Carrier {
        match self.carrier.to_lowercase().as_str() {
            "ups" => Carrier::UPS,
            "fedex" => Carrier::FedEx,
            "usps" => Carrier::USPS,
            "dhl" => Carrier::DHL,
            "canada_post" | "canadapost" => Carrier::CanadaPost,
            "ontrac" => Carrier::OnTrac,
            _ => Carrier::detect(&self.tracking_number),
        }
    }

    pub fn days_until_delivery(&self) -> Option<i64> {
        self.tracking_info
            .as_ref()
            .and_then(|info| info.estimated_delivery_date)
            .map(|eta| {
                let today = Utc::now().date_naive();
                let est_date = eta.date_naive();
                (est_date - today).num_days()
            })
    }
}

// 17track API Response types
#[derive(Debug, Deserialize)]
pub struct Track17Response<T> {
    pub code: i32,
    pub data: T,
}

#[derive(Debug, Deserialize)]
pub struct Track17Data {
    #[serde(default)]
    pub accepted: Vec<Track17Item>,
    #[serde(default)]
    pub rejected: Vec<Track17Rejected>,
}

#[derive(Debug, Deserialize)]
pub struct Track17Item {
    pub number: String,
    pub carrier: Option<i32>,
    pub track_info: Track17TrackInfo,
}

#[derive(Debug, Deserialize)]
pub struct Track17Rejected {
    pub number: String,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Track17TrackInfo {
    pub tracking: Option<Track17Tracking>,
    pub latest_status: Option<Track17LatestStatus>,
    pub latest_event: Option<Track17Event>,
    #[serde(default)]
    pub shipping_info: Track17ShippingInfo,
    #[serde(default)]
    pub time_details: Track17TimeDetails,
}

#[derive(Debug, Deserialize)]
pub struct Track17Tracking {
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Track17LatestStatus {
    pub status: Option<String>,
    #[serde(default)]
    pub sub_status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Track17Event {
    pub description: Option<String>,
    pub location: Option<String>,
    pub time: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Track17ShippingInfo {
    pub shipping_date: Option<String>,
    pub weight: Option<String>,
    pub service: Option<String>,
    pub shipper_address: Option<Track17Address>,
    pub recipient_address: Option<Track17Address>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Track17Address {
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Track17TimeDetails {
    pub date_delivered: Option<String>,
    pub date_estimated: Option<String>,
}

// Alternative data structure for error responses
#[derive(Debug, Deserialize)]
pub struct Track17DataFlexible {
    #[serde(default)]
    pub accepted: Vec<Track17Item>,
    #[serde(default)]
    pub rejected: Vec<Track17Rejected>,
    #[serde(default)]
    pub errors: Vec<Track17Error>,
}

#[derive(Debug, Deserialize)]
pub struct Track17Error {
    pub code: i32,
    pub message: String,
}

// API Response types for simpler parsing
pub struct ApiResponse {
    pub accepted: Vec<TrackInfoAccepted>,
    pub rejected: Vec<TrackInfoRejected>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrackInfoAccepted {
    pub number: String,
    pub carrier: i32,
    pub track_info: TrackInfo,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrackInfoRejected {
    pub number: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrackInfo {
    pub tracking: Option<TrackDetail>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrackDetail {
    pub track_status: Option<String>,
    pub track_status_name: Option<String>,
    pub estimated_delivery_date: Option<String>,
    pub carrier_status: Option<String>,
    pub carrier_status_code: Option<String>,
    pub track_z0: Option<TrackLocation>,
    pub track_z1: Option<TrackLocation>,
    pub track_z2: Option<TrackLocation>,
    pub track_z3: Option<TrackLocation>,
    pub track_z4: Option<TrackLocation>,
    pub track_z5: Option<TrackLocation>,
    pub track_z6: Option<TrackLocation>,
    pub track_z7: Option<TrackLocation>,
    pub track_z8: Option<TrackLocation>,
    pub track_z9: Option<TrackLocation>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrackLocation {
    pub z: Option<String>,
    pub t: Option<String>,
}
