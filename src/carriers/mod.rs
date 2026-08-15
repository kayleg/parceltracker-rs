// Tracking providers beyond the 17track aggregator. Amazon Logistics
// (TBA…) has no aggregator coverage but does have a public
// recipient-facing endpoint, so those parcels are always served
// first-party with no credentials; every other carrier goes through
// 17track (src/api.rs).
//
// A provider returns a plain TrackingInfo whose status_code uses the
// same vocabulary 17track does ("InTransit", "OutForDelivery",
// "Delivered", "InfoReceived", "Exception") — Parcel::status_state()
// classifies by sniffing that text, so emitting the shared vocabulary is
// what keeps the TUI and Omarchy widget working unchanged.

pub mod amazon;

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::models::{Carrier, Config, TrackingInfo};

pub enum FirstParty {
    Amazon(amazon::Amazon),
}

impl FirstParty {
    pub fn name(&self) -> &'static str {
        match self {
            FirstParty::Amazon(_) => "Amazon",
        }
    }

    pub async fn track(
        &self,
        http: &reqwest::Client,
        tracking_number: &str,
    ) -> Result<TrackingInfo> {
        match self {
            FirstParty::Amazon(p) => p.track(http, tracking_number).await,
        }
    }
}

/// The first-party provider for a carrier, if one exists.
pub fn for_carrier(carrier: &Carrier, _config: &Config) -> Option<FirstParty> {
    match carrier {
        Carrier::Amazon => Some(FirstParty::Amazon(amazon::Amazon)),
        _ => None,
    }
}

/// Carrier APIs mix RFC3339-with-offset, naive datetimes, and bare dates
/// depending on field. Naive values are taken as UTC — ETAs are consumed
/// at day granularity, so the shortcut is harmless.
pub(crate) fn parse_carrier_datetime(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(naive.and_utc());
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return date.and_hms_opt(23, 59, 0).map(|n| n.and_utc());
    }
    None
}

/// "CITY, ST, CC" from optional address parts, matching the location
/// format 17track uses (which the widget's geocoder already handles).
pub(crate) fn join_location(parts: &[Option<&str>]) -> String {
    parts
        .iter()
        .filter_map(|p| *p)
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datetime_forms() {
        assert!(parse_carrier_datetime("2026-08-16T14:00:00-05:00").is_some());
        assert!(parse_carrier_datetime("2026-08-16T14:00:00").is_some());
        let d = parse_carrier_datetime("2026-08-16").unwrap();
        assert_eq!(d.to_rfc3339(), "2026-08-16T23:59:00+00:00");
        assert!(parse_carrier_datetime("tomorrow").is_none());
    }

    #[test]
    fn location_joins_present_parts() {
        assert_eq!(
            join_location(&[Some("ORANGEBURG"), Some("SC"), Some("US")]),
            "ORANGEBURG, SC, US"
        );
        assert_eq!(join_location(&[Some("Memphis"), None, Some("US")]), "Memphis, US");
        assert_eq!(join_location(&[None, Some("")]), "");
    }
}
