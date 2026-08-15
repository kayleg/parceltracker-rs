use anyhow::Result;
use chrono::Utc;


use crate::models::{Parcel, BarSelection};
use crate::storage::{load_config, save_config};

pub fn format_eta_smart(eta_str: &str) -> String {
    use chrono::{DateTime, Datelike, Local, Weekday};

    let datetime = match DateTime::parse_from_rfc3339(eta_str) {
        Ok(dt) => dt.with_timezone(&Local),
        Err(_) => return eta_str.to_string(),
    };

    let now = Local::now();
    let today = now.date_naive();
    let delivery_date = datetime.date_naive();
    let days_diff = (delivery_date - today).num_days();
    let time_str = datetime.format("%-I:%M%p").to_string().to_lowercase();

    match days_diff {
        0 => format!("Today @ {}", time_str),
        1 => format!("Tomorrow @ {}", time_str),
        2..=6 => {
            let day_name = match datetime.weekday() {
                Weekday::Mon => "Mon",
                Weekday::Tue => "Tue",
                Weekday::Wed => "Wed",
                Weekday::Thu => "Thu",
                Weekday::Fri => "Fri",
                Weekday::Sat => "Sat",
                Weekday::Sun => "Sun",
            };
            format!("{} @ {}", day_name, time_str)
        }
        7..=365 => datetime
            .format("%b %d @ %-I:%M%p")
            .to_string()
            .to_lowercase(),
        _ => datetime
            .format("%b %d, %Y @ %-I:%M%p")
            .to_string()
            .to_lowercase(),
    }
}

pub fn resolve_bar_parcel(parcels: &[Parcel]) -> Result<Option<&Parcel>> {
    let config = load_config()?;
    let selected = if let Some(selection) = &config.bar_selected {
        parcels
            .iter()
            .find(|p| p.tracking_number == selection.tracking)
    } else {
        None
    };
    Ok(selected.or_else(|| find_first_arriving(parcels)))
}

pub fn select_parcel_for_bar(parcels: &[Parcel], identifier: &str) -> Result<String> {
    // Try position first
    let parcel_tracking = if let Ok(pos) = identifier.parse::<usize>() {
        if pos > 0 && pos <= parcels.len() {
            Some(parcels[pos - 1].tracking_number.clone())
        } else {
            None
        }
    } else {
        // Try tracking number
        parcels
            .iter()
            .find(|p| p.tracking_number == identifier)
            .map(|p| p.tracking_number.clone())
    };

    if let Some(tracking) = parcel_tracking {
        let parcel = parcels
            .iter()
            .find(|p| p.tracking_number == tracking)
            .unwrap();

        let selection = BarSelection {
            tracking: tracking.clone(),
            timestamp: Utc::now().to_rfc3339(),
        };

        let mut config = load_config()?;
        config.bar_selected = Some(selection);
        save_config(&config)?;

        Ok(format!("Selected: {}", parcel.display_name()))
    } else {
        Ok(format!("Parcel not found: {}", identifier))
    }
}

pub fn unselect_parcel() -> Result<String> {
    let mut config = load_config()?;
    config.bar_selected = None;
    save_config(&config)?;
    Ok("Selection cleared".to_string())
}

fn find_first_arriving(parcels: &[Parcel]) -> Option<&Parcel> {
    let mut undelivered: Vec<&Parcel> = parcels.iter().filter(|p| !p.is_delivered()).collect();

    // Sort by estimated delivery date
    undelivered.sort_by(
        |a, b| match (a.days_until_delivery(), b.days_until_delivery()) {
            (Some(a_days), Some(b_days)) => a_days.cmp(&b_days),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        },
    );

    undelivered.first().copied()
}
