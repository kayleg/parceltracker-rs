use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::models::{Cache, Config, Parcel};

pub fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("parceltracker")
}

pub fn ensure_data_dir() -> Result<PathBuf> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn load_parcels() -> Result<Vec<Parcel>> {
    let path = data_dir().join("parcels.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path)?;
    let parcels: Vec<Parcel> = serde_json::from_str(&content)?;
    Ok(parcels)
}

pub fn save_parcels(parcels: &[Parcel]) -> Result<()> {
    ensure_data_dir()?;
    let path = data_dir().join("parcels.json");
    let content = serde_json::to_string_pretty(parcels)?;
    std::fs::write(&path, content)?;
    Ok(())
}

pub fn load_config() -> Result<Config> {
    let path = data_dir().join("config.json");
    if !path.exists() {
        return Ok(Config::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let config: Config = serde_json::from_str(&content)?;
    Ok(config)
}

pub fn save_config(config: &Config) -> Result<()> {
    ensure_data_dir()?;
    let path = data_dir().join("config.json");
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}

pub fn load_cache() -> Result<Cache> {
    let path = data_dir().join("cache.json");
    if !path.exists() {
        return Ok(Cache::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let cache: Cache = serde_json::from_str(&content)?;
    Ok(cache)
}

pub fn save_cache(cache: &Cache) -> Result<()> {
    ensure_data_dir()?;
    let path = data_dir().join("cache.json");
    let content = serde_json::to_string_pretty(cache)?;
    std::fs::write(&path, content)?;
    Ok(())
}

pub fn find_parcel_by_position(parcels: &[Parcel], position: usize) -> Option<&Parcel> {
    if position == 0 {
        None
    } else {
        parcels.get(position - 1)
    }
}

pub fn find_parcel_by_tracking<'a>(parcels: &'a [Parcel], tracking: &str) -> Option<&'a Parcel> {
    parcels.iter().find(|p| p.tracking_number == tracking)
}

pub fn remove_parcel(parcels: &mut Vec<Parcel>, identifier: &str) -> Result<Option<Parcel>> {
    // Try position first
    if let Ok(pos) = identifier.parse::<usize>() {
        if pos > 0 && pos <= parcels.len() {
            return Ok(Some(parcels.remove(pos - 1)));
        }
    }

    // Try tracking number
    if let Some(index) = parcels.iter().position(|p| p.tracking_number == identifier) {
        return Ok(Some(parcels.remove(index)));
    }

    Ok(None)
}

pub fn rename_parcel(
    parcels: &mut [Parcel],
    identifier: &str,
    new_description: &str,
) -> Result<bool> {
    // Try position first
    if let Ok(pos) = identifier.parse::<usize>() {
        if pos > 0 && pos <= parcels.len() {
            parcels[pos - 1].description = new_description.to_string();
            return Ok(true);
        }
    }

    // Try tracking number
    if let Some(parcel) = parcels.iter_mut().find(|p| p.tracking_number == identifier) {
        parcel.description = new_description.to_string();
        return Ok(true);
    }

    Ok(false)
}

pub fn find_parcel_for_selection(parcels: &[Parcel], identifier: &str) -> Option<String> {
    // Try position first
    if let Ok(pos) = identifier.parse::<usize>() {
        if pos > 0 && pos <= parcels.len() {
            return Some(parcels[pos - 1].id.clone());
        }
    }

    // Try tracking number
    parcels
        .iter()
        .find(|p| p.tracking_number == identifier)
        .map(|p| p.id.clone())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarCacheEntry {
    pub parcel_id: String,
    pub delivered_at: Option<DateTime<Utc>>,
    pub cached_at: DateTime<Utc>,
}

pub fn should_clear_selection(_selection: &crate::models::BarSelection) -> bool {
    // For now, always return false - no expiration on the bar selection
    // Could be enhanced later with time-based expiration
    false
}
