use crate::models::{Carrier, Parcel, TrackingInfo};
use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use regex::Regex;
use std::time::Duration;

lazy_static! {
    static ref UPS_REGEX: Regex = Regex::new(r"^1Z[A-Z0-9]{16}$").unwrap();
    static ref FEDEX_REGEX: Regex = Regex::new(r"^\d{12}$|^\d{15}$").unwrap();
    static ref USPS_REGEX: Regex = Regex::new(r"^\d{20,22}$|^9400\d{16,}$").unwrap();
    static ref DHL_REGEX: Regex = Regex::new(r"^\d{10}$|^JJD\d{12,}$").unwrap();
    static ref CANADA_POST_REGEX: Regex = Regex::new(r"^[A-Z]{2}\d{9}CA$").unwrap();
    static ref ONTRAC_REGEX: Regex = Regex::new(r"^C\d{14}$|^D\d{14}$").unwrap();
}

pub fn detect_carrier(tracking_number: &str) -> Option<Carrier> {
    let normalized = tracking_number.to_uppercase().replace(" ", "");

    if UPS_REGEX.is_match(&normalized) {
        return Some(Carrier::UPS);
    }
    if FEDEX_REGEX.is_match(&normalized) {
        return Some(Carrier::FedEx);
    }
    if USPS_REGEX.is_match(&normalized) {
        return Some(Carrier::USPS);
    }
    if DHL_REGEX.is_match(&normalized) {
        return Some(Carrier::DHL);
    }
    if CANADA_POST_REGEX.is_match(&normalized) {
        return Some(Carrier::CanadaPost);
    }
    if ONTRAC_REGEX.is_match(&normalized) {
        return Some(Carrier::OnTrac);
    }
    None
}

pub fn get_carrier_code(carrier: &Carrier) -> i32 {
    match carrier {
        Carrier::UPS => 100002,
        Carrier::FedEx => 100003,
        Carrier::USPS => 100009,
        Carrier::DHL => 100004,
        Carrier::CanadaPost => 100007,
        Carrier::OnTrac => 100008,
        Carrier::Unknown => 0,
    }
}

pub async fn register_parcels(
    http_client: &reqwest::Client,
    api_key: &str,
    parcels: &[Parcel],
) -> Result<()> {
    let url = "https://api.17track.net/track/v2.4/register";

    // Parse carriers and build requests
    let mut requests = Vec::new();
    for p in parcels {
        let carrier = match p.carrier.as_str() {
            "ups" => Carrier::UPS,
            "fedex" => Carrier::FedEx,
            "usps" => Carrier::USPS,
            "dhl" => Carrier::DHL,
            "canada_post" | "canadapost" => Carrier::CanadaPost,
            "ontrac" => Carrier::OnTrac,
            "auto" => Carrier::detect(&p.tracking_number),
            _ => Carrier::detect(&p.tracking_number),
        };

        if carrier != Carrier::Unknown {
            requests.push(serde_json::json!({
                "number": p.tracking_number,
                "carrier": get_carrier_code(&carrier)
            }));
        }
    }

    if requests.is_empty() {
        return Ok(());
    }

    let response = http_client
        .post(url)
        .header("Content-Type", "application/json")
        .header("17token", api_key)
        .json(&requests)
        .send()
        .await?;

    let text = response.text().await?;
    let json_val: serde_json::Value = serde_json::from_str(&text)?;

    let code = json_val.get("code").and_then(|c| c.as_i64()).unwrap_or(0) as i32;
    if code != 0 {
        let message = json_val
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Registration failed");
        return Err(anyhow!("API error: {}", message));
    }

    Ok(())
}

pub fn get_tracking_url(carrier: &Carrier, tracking_number: &str) -> Option<String> {
    match carrier {
        Carrier::UPS => Some(format!(
            "https://www.ups.com/track?tracknum={}",
            tracking_number
        )),
        Carrier::FedEx => Some(format!(
            "https://www.fedex.com/fedextrack/?trknbr={}",
            tracking_number
        )),
        Carrier::USPS => Some(format!(
            "https://tools.usps.com/go/TrackConfirmAction?tLabels={}",
            tracking_number
        )),
        Carrier::DHL => Some(format!(
            "https://www.dhl.com/en/express/tracking.html?AWB={}",
            tracking_number
        )),
        Carrier::CanadaPost => Some(format!(
            "https://www.canadapost-postescanada.ca/track-reperage/en#/search?searchFor={}",
            tracking_number
        )),
        Carrier::OnTrac => Some(format!(
            "https://www.ontrac.com/tracking.asp?tracking_number={}",
            tracking_number
        )),
        Carrier::Unknown => None,
    }
}

pub struct Client {
    http_client: reqwest::Client,
    api_key: String,
}

impl Client {
    pub async fn new() -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        // Try to load API key from config
        let api_key = crate::storage::load_config()?
            .track17_api_key
            .unwrap_or_default();

        Ok(Self {
            http_client,
            api_key,
        })
    }

    /// Register parcels with 17track. This is required before `gettrackinfo`
    /// will return any data for a number. Numbers that are already registered
    /// are reported per-number by the API (top-level code stays 0) and ignored.
    pub async fn register(&self, parcels: &[Parcel]) -> Result<()> {
        if self.api_key.is_empty() {
            return Ok(());
        }
        register_parcels(&self.http_client, &self.api_key, parcels).await
    }

    pub async fn get_tracking_info(&self, parcel: &Parcel) -> Result<TrackingInfo> {
        if self.api_key.is_empty() {
            // Return placeholder if no API key
            return Ok(TrackingInfo {
                status_code: "pending".to_string(),
                status_text: Some("Pending (no API key)".to_string()),
                current_location: None,
                estimated_delivery_date: None,
                events: Vec::new(),
                raw_data: None,
            });
        }

        let url = "https://api.17track.net/track/v2.4/gettrackinfo";

        // Parse carrier
        let carrier = match parcel.carrier.as_str() {
            "ups" => Carrier::UPS,
            "fedex" => Carrier::FedEx,
            "usps" => Carrier::USPS,
            "dhl" => Carrier::DHL,
            "canada_post" | "canadapost" => Carrier::CanadaPost,
            "ontrac" => Carrier::OnTrac,
            "auto" => Carrier::detect(&parcel.tracking_number),
            _ => Carrier::detect(&parcel.tracking_number),
        };

        if carrier == Carrier::Unknown {
            return Err(anyhow!(
                "Could not detect carrier for {}",
                parcel.tracking_number
            ));
        }

        let request = serde_json::json!([{
            "number": parcel.tracking_number,
            "carrier": get_carrier_code(&carrier)
        }]);

        let response = self
            .http_client
            .post(url)
            .header("Content-Type", "application/json")
            .header("17token", &self.api_key)
            .json(&request)
            .send()
            .await?;

        let text = response.text().await?;
        let json_val: serde_json::Value = serde_json::from_str(&text)?;

        let code = json_val.get("code").and_then(|c| c.as_i64()).unwrap_or(-1) as i32;

        // Check for errors in data.errors even when code is 0
        if let Some(errors) = json_val
            .get("data")
            .and_then(|d| d.get("errors"))
            .and_then(|e| e.as_array())
        {
            if !errors.is_empty() {
                let error_msg = errors[0]
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("API error");
                return Err(anyhow!("API error: {}", error_msg));
            }
        }

        // Surface rejected numbers (e.g. not registered, or still being queried
        // by 17track) instead of silently returning a "no info" placeholder.
        if let Some(rejected) = json_val
            .get("data")
            .and_then(|d| d.get("rejected"))
            .and_then(|r| r.as_array())
        {
            if let Some(item) = rejected.iter().find(|it| {
                it.get("number").and_then(|n| n.as_str()) == Some(parcel.tracking_number.as_str())
            }) {
                let msg = item
                    .get("error")
                    .and_then(|e| {
                        e.get("message")
                            .and_then(|m| m.as_str())
                            .or_else(|| e.as_str())
                    })
                    .unwrap_or("number rejected by 17track");
                return Err(anyhow!("Rejected by 17track: {}", msg));
            }
        }

        if code == 0 {
            // Success - extract data.accepted
            if let Some(accepted) = json_val
                .get("data")
                .and_then(|d| d.get("accepted"))
                .and_then(|a| a.as_array())
            {
                if let Some(item) = accepted.first() {
                    if let Some(track_info) = item.get("track_info") {
                        let status = track_info
                            .get("latest_status")
                            .and_then(|ls| ls.get("status"))
                            .and_then(|s| s.as_str())
                            .or_else(|| {
                                track_info
                                    .get("tracking")
                                    .and_then(|t| t.get("status"))
                                    .and_then(|s| s.as_str())
                            })
                            .unwrap_or("transit")
                            .to_string();

                        // Extract ETA using time_metrics.estimated_delivery_date.to first, then fallback
                        let eta_str = track_info
                            .get("time_metrics")
                            .and_then(|tm| tm.get("estimated_delivery_date"))
                            .and_then(|edd| edd.get("to"))
                            .and_then(|t| t.as_str())
                            .or_else(|| {
                                track_info
                                    .get("time_details")
                                    .and_then(|td| td.get("date_estimated"))
                                    .and_then(|d| d.as_str())
                            });

                        let eta = eta_str.and_then(|s| {
                            chrono::DateTime::parse_from_rfc3339(s)
                                .map(|dt| dt.with_timezone(&chrono::Utc))
                                .ok()
                                .or_else(|| {
                                    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                                        .ok()
                                        .map(|d| {
                                            d.and_hms_opt(0, 0, 0)
                                                .unwrap()
                                                .and_local_timezone(chrono::Utc)
                                                .unwrap()
                                        })
                                })
                        });

                        return Ok(TrackingInfo {
                            status_code: status.clone(),
                            status_text: Some(status),
                            current_location: track_info
                                .get("latest_event")
                                .and_then(|le| le.get("location"))
                                .and_then(|l| l.as_str())
                                .map(|s| s.to_string()),
                            estimated_delivery_date: eta,
                            events: Vec::new(),
                            raw_data: Some(track_info.clone()),
                        });
                    }
                }
            }

            // If we get here, no tracking info found
            Ok(TrackingInfo {
                status_code: "pending".to_string(),
                status_text: Some("No tracking info available".to_string()),
                current_location: None,
                estimated_delivery_date: None,
                events: Vec::new(),
                raw_data: None,
            })
        } else {
            let message = json_val
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown API error");
            Err(anyhow!("API error {}: {}", code, message))
        }
    }
}
