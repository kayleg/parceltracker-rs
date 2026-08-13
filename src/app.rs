use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parcel {
    pub tracking: String,
    pub carrier: String,
    pub description: String,
    pub status: String,
    pub last_checked: u64,
    pub tracking_events: Vec<TrackingEvent>,
    pub estimated_delivery: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingEvent {
    pub timestamp: String,
    pub status: String,
    pub location: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub track17_api_key: Option<String>,
    pub waybar_selected: Option<WaybarSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaybarSelection {
    pub tracking: String,
    pub timestamp: u64,
}

pub struct App {
    config_dir: PathBuf,
    cache_dir: PathBuf,
}

impl App {
    pub fn new() -> Self {
        let home = dirs::home_dir().expect("Could not find home directory");
        let config_dir = home.join(".config").join("parceltracker");
        let cache_dir = home.join(".cache").join("parceltracker");

        fs::create_dir_all(&config_dir).ok();
        fs::create_dir_all(&cache_dir).ok();

        Self {
            config_dir,
            cache_dir,
        }
    }

    fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.json")
    }

    fn parcels_file(&self) -> PathBuf {
        self.cache_dir.join("parcels.json")
    }

    fn cache_file(&self) -> PathBuf {
        self.cache_dir.join("tracking_cache.json")
    }

    pub fn load_config(&self) -> Config {
        match fs::read_to_string(self.config_file()) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| Config {
                track17_api_key: None,
                waybar_selected: None,
            }),
            Err(_) => Config {
                track17_api_key: None,
                waybar_selected: None,
            },
        }
    }

    pub fn save_config(&self, config: &Config) -> io::Result<()> {
        let content = serde_json::to_string_pretty(config)?;
        fs::write(self.config_file(), content)
    }

    pub fn load_parcels(&self) -> Vec<Parcel> {
        match fs::read_to_string(self.parcels_file()) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => vec![],
        }
    }

    pub fn save_parcels(&self, parcels: &[Parcel]) -> io::Result<()> {
        let content = serde_json::to_string_pretty(parcels)?;
        fs::write(self.parcels_file(), content)
    }

    pub fn add_parcel(&self, tracking: &str, carrier: &str, description: &str) {
        let mut parcels = self.load_parcels();

        if parcels.iter().any(|p| p.tracking == tracking) {
            println!("Parcel {} is already being tracked", tracking);
            return;
        }

        let parcel = Parcel {
            tracking: tracking.to_string(),
            carrier: carrier.to_string(),
            description: description.to_string(),
            status: "Info Received".to_string(),
            last_checked: current_timestamp(),
            tracking_events: vec![],
            estimated_delivery: None,
        };

        parcels.push(parcel);
        self.save_parcels(&parcels).unwrap();
        println!("Added parcel: {} ({})", tracking, description);
    }

    pub fn remove_parcel(&self, identifier: &str) {
        let mut parcels = self.load_parcels();

        // Try to parse as position
        if let Ok(pos) = identifier.parse::<usize>() {
            if pos == 0 || pos > parcels.len() {
                eprintln!("Position {} out of range (1-{})", pos, parcels.len());
                std::process::exit(1);
            }
            let removed = parcels.remove(pos - 1);
            self.save_parcels(&parcels).unwrap();
            println!(
                "Removed parcel: {} ({})",
                removed.tracking, removed.description
            );
            return;
        }

        // Remove by tracking number
        let before = parcels.len();
        parcels.retain(|p| p.tracking != identifier);

        if parcels.len() == before {
            eprintln!("Parcel {} not found", identifier);
            std::process::exit(1);
        }

        self.save_parcels(&parcels).unwrap();
        println!("Removed parcel: {}", identifier);
    }

    pub fn rename_parcel(&self, identifier: &str, new_description: &str) {
        let mut parcels = self.load_parcels();

        // Try to parse as position
        if let Ok(pos) = identifier.parse::<usize>() {
            if pos == 0 || pos > parcels.len() {
                eprintln!("Position {} out of range (1-{})", pos, parcels.len());
                std::process::exit(1);
            }
            let old_desc = parcels[pos - 1].description.clone();
            parcels[pos - 1].description = new_description.to_string();
            self.save_parcels(&parcels).unwrap();
            println!(
                "Renamed parcel {}: '{}' -> '{}'",
                parcels[pos - 1].tracking,
                old_desc,
                new_description
            );
            return;
        }

        // Rename by tracking number
        if let Some(parcel) = parcels.iter_mut().find(|p| p.tracking == identifier) {
            let old_desc = parcel.description.clone();
            parcel.description = new_description.to_string();
            self.save_parcels(&parcels).unwrap();
            println!(
                "Renamed parcel {}: '{}' -> '{}'",
                identifier, old_desc, new_description
            );
        } else {
            eprintln!("Parcel {} not found", identifier);
            std::process::exit(1);
        }
    }

    pub fn select_waybar(&self, identifier: &str) {
        let parcels = self.load_parcels();
        let mut config = self.load_config();

        let tracking = if let Ok(pos) = identifier.parse::<usize>() {
            if pos == 0 || pos > parcels.len() {
                eprintln!("Position {} out of range (1-{})", pos, parcels.len());
                std::process::exit(1);
            }
            parcels[pos - 1].tracking.clone()
        } else {
            if !parcels.iter().any(|p| p.tracking == identifier) {
                eprintln!("Parcel {} not found", identifier);
                std::process::exit(1);
            }
            identifier.to_string()
        };

        config.waybar_selected = Some(WaybarSelection {
            tracking: tracking.clone(),
            timestamp: current_timestamp(),
        });

        self.save_config(&config).unwrap();
        println!("Selected parcel for waybar: {}", tracking);
    }

    pub fn unselect_waybar(&self) {
        let mut config = self.load_config();
        config.waybar_selected = None;
        self.save_config(&config).unwrap();
        println!("Cleared waybar selection");
    }

    pub fn set_api_key(&self, key: &str) {
        let mut config = self.load_config();
        config.track17_api_key = Some(key.to_string());
        self.save_config(&config).unwrap();
        println!("API key saved");
    }
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn format_delivery_date(estimated: &Option<String>) -> String {
    if let Some(date_str) = estimated {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            let today = chrono::Local::now().naive_local().date();
            let days = (date - today).num_days();

            match days {
                n if n < 0 => " (delivered)".to_string(),
                0 => " (today)".to_string(),
                1 => " (1 day)".to_string(),
                n => format!(" ({} days)", n),
            }
        } else {
            format!(" ({})", date_str)
        }
    } else {
        String::new()
    }
}
