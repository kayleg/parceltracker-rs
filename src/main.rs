use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Table};
use std::process::Command;

#[allow(dead_code)]
mod api;
mod carriers;
mod jsonout;
mod setup;
#[allow(dead_code)]
mod models;
#[allow(dead_code)]
mod storage;
mod tui;
mod waybar;

use api::Client as ApiClient;
use models::{Carrier, Parcel};
use storage::{load_config, load_parcels, remove_parcel, rename_parcel, save_parcels};
use waybar::{get_waybar_output, resolve_waybar_parcel, select_parcel_for_waybar, unselect_parcel};

#[derive(Parser)]
#[command(name = "parceltracker")]
#[command(about = "Track parcels with 17track API")]
#[command(version = "1.0.0")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch interactive TUI (default)
    Tui,

    /// Add a new parcel
    Add {
        /// Tracking number
        tracking: String,

        /// Optional description
        description: Option<String>,
    },

    /// Remove a parcel
    Remove {
        /// Position number or tracking number
        identifier: String,
    },

    /// Rename a parcel
    Rename {
        /// Position number or tracking number
        identifier: String,

        /// New description
        new_description: String,
    },

    /// List all parcels
    List,

    /// Update tracking information from API
    Update,

    /// Select the parcel that leads the bar display
    Select {
        /// Position number or tracking number
        identifier: String,
    },

    /// Clear the bar's parcel selection
    Unselect,

    /// Output waybar JSON
    Waybar,

    /// Open tracking page in browser (defaults to the bar-selected parcel)
    Open {
        /// Position number or tracking number (optional)
        identifier: Option<String>,
    },

    /// Cycle the bar's selected parcel
    Cycle,

    /// Status output (for waybar compatibility)
    Status {
        /// Output waybar JSON format
        #[arg(long)]
        waybar: bool,

        /// Output the full machine-readable status document
        #[arg(long, conflicts_with = "waybar")]
        json: bool,
    },

    /// Interactive setup: walks through getting a key for each tracking
    /// provider, validates it live, and saves the configuration
    Setup,

    /// Show or set API credentials. Amazon (TBA…) parcels need no key;
    /// everything else uses 17track. With no flags, shows what is
    /// configured.
    Config {
        /// 17track API key (api.17track.net)
        #[arg(long)]
        track17_key: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Tui) => run_tui().await,
        Some(Commands::Add {
            tracking,
            description,
        }) => add_parcel(tracking, description.unwrap_or_default()).await,
        Some(Commands::Remove { identifier }) => remove_parcel_cmd(&identifier).await,
        Some(Commands::Rename {
            identifier,
            new_description,
        }) => rename_parcel_cmd(&identifier, &new_description).await,
        Some(Commands::List) => list_parcels().await,
        Some(Commands::Update) => update_parcels().await,
        Some(Commands::Select { identifier }) => select_for_waybar(&identifier).await,
        Some(Commands::Unselect) => unselect_for_waybar().await,
        Some(Commands::Waybar) => output_waybar().await,
        Some(Commands::Open { identifier }) => open_tracking(identifier.as_deref()).await,
        Some(Commands::Cycle) => cycle_waybar_selection().await,
        Some(Commands::Setup) => setup::run().await,
        Some(Commands::Config { track17_key }) => config_cmd(track17_key),
        Some(Commands::Status { waybar, json }) => {
            if json {
                output_json().await
            } else if waybar {
                output_waybar().await
            } else {
                list_parcels().await
            }
        }
    }
}

async fn run_tui() -> Result<()> {
    let parcels = load_parcels()?;

    if parcels.is_empty() {
        println!(
            "{}",
            "No parcels tracked. Use 'parceltracker add <tracking> [description]' to add one."
                .yellow()
        );
        return Ok(());
    }

    let mut app = tui::App::new(parcels)?;
    app.run()?;

    Ok(())
}

async fn add_parcel(tracking: String, description: String) -> Result<()> {
    let mut parcels = load_parcels()?;

    // Check for duplicates
    if parcels.iter().any(|p| p.tracking_number == tracking) {
        println!(
            "{}",
            format!("Parcel with tracking number '{}' already exists.", tracking).yellow()
        );
        return Ok(());
    }

    let carrier = Carrier::detect(&tracking);
    let parcel = Parcel::new(
        tracking.clone(),
        description.clone(),
        carrier.name().to_lowercase(),
    );

    parcels.push(parcel);
    save_parcels(&parcels)?;

    let display_desc = if description.is_empty() {
        tracking.clone()
    } else {
        description.clone()
    };

    println!(
        "{} Added: {} ({} - {})",
        "✓".green().bold(),
        display_desc.bold(),
        tracking.cyan(),
        carrier.name().yellow()
    );

    // Optionally register with API immediately
    println!("Run 'parceltracker update' to fetch tracking information.");

    Ok(())
}

async fn remove_parcel_cmd(identifier: &str) -> Result<()> {
    let mut parcels = load_parcels()?;

    match remove_parcel(&mut parcels, identifier)? {
        Some(parcel) => {
            save_parcels(&parcels)?;
            println!(
                "{} Removed: {} ({})",
                "✓".green().bold(),
                parcel.display_name().bold(),
                parcel.tracking_number.cyan()
            );
        }
        None => {
            println!(
                "{} Parcel not found: {}",
                "✗".red().bold(),
                identifier.yellow()
            );
        }
    }

    Ok(())
}

async fn rename_parcel_cmd(identifier: &str, new_description: &str) -> Result<()> {
    let mut parcels = load_parcels()?;

    if rename_parcel(&mut parcels, identifier, new_description)? {
        save_parcels(&parcels)?;
        println!(
            "{} Renamed to: {}",
            "✓".green().bold(),
            new_description.bold()
        );
    } else {
        println!(
            "{} Parcel not found: {}",
            "✗".red().bold(),
            identifier.yellow()
        );
    }

    Ok(())
}

async fn list_parcels() -> Result<()> {
    let parcels = load_parcels()?;

    if parcels.is_empty() {
        println!("{}", "No parcels tracked.".yellow());
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec![
        "#",
        "Status",
        "Description",
        "Tracking",
        "Carrier",
        "ETA",
    ]);
    table.apply_modifier(UTF8_FULL);
    table.apply_modifier(UTF8_ROUND_CORNERS);

    for (i, parcel) in parcels.iter().enumerate() {
        let status_text = parcel
            .tracking_info
            .as_ref()
            .map(|t| t.status_text())
            .unwrap_or_else(|| "Not updated".to_string());
        let status = format!("{} {}", parcel.status_emoji(), status_text);

        let eta = parcel
            .tracking_info
            .as_ref()
            .and_then(|info| info.estimated_delivery_date.as_ref())
            .map(|eta| format_eta_smart(&eta.to_rfc3339()))
            .unwrap_or_else(|| "-".to_string());

        let carrier = if parcel.carrier == "auto" {
            Carrier::detect(&parcel.tracking_number).name().to_string()
        } else {
            parcel.carrier.clone()
        };

        table.add_row(vec![
            (i + 1).to_string(),
            status,
            parcel.description.clone(),
            parcel.tracking_number.clone(),
            carrier,
            eta,
        ]);
    }

    println!("{}", table);
    println!("\nTotal: {} parcel(s)", parcels.len());

    fn format_eta_smart(eta_str: &str) -> String {
        use chrono::{DateTime, Datelike, Local, Timelike, Weekday};

        // Parse the datetime string with timezone
        let datetime = match DateTime::parse_from_rfc3339(eta_str) {
            Ok(dt) => dt.with_timezone(&Local),
            Err(_) => return eta_str.to_string(), // Return original if parsing fails
        };

        let now = Local::now();
        let today = now.date_naive();
        let delivery_date = datetime.date_naive();

        let days_diff = (delivery_date - today).num_days();

        let time_str = if datetime.minute() == 0 {
            datetime.format("%-I%P").to_string()
        } else {
            datetime.format("%-I:%M%P").to_string()
        };

        match days_diff {
            0 => format!("Today @ {}", time_str),
            1 => format!("Tomorrow @ {}", time_str),
            2..=6 => {
                // This week - show day name
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
            7..=365 => {
                // Within a year - show Month Day
                datetime
                    .format("%b %d @ %-I:%M%p")
                    .to_string()
                    .to_lowercase()
            }
            _ => {
                // Over a year - show full date
                datetime
                    .format("%b %d, %Y @ %-I:%M%p")
                    .to_string()
                    .to_lowercase()
            }
        }
    }

    Ok(())
}

async fn update_parcels() -> Result<()> {
    let mut parcels = load_parcels()?;

    if parcels.is_empty() {
        println!("{}", "No parcels to update.".yellow());
        return Ok(());
    }

    println!("{} Fetching tracking information...", "⟳".cyan());

    let config = load_config().unwrap_or_default();
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let client = ApiClient::new().await?;

    // Register numbers that the 17track tier will serve. gettrackinfo only
    // returns data for registered numbers, so a freshly-added parcel must be
    // registered before its status can be fetched. First-party parcels are
    // registered too: they fall back to 17track when the carrier API errors.
    if let Err(e) = client.register(&parcels).await {
        eprintln!("  {} Registration warning: {}", "⚠".yellow(), e);
    } else {
        // Give 17track a moment to process the registration before fetching.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    for parcel in parcels.iter_mut() {
        print!("  {}... ", parcel.tracking_number.cyan());

        // Tier 1: the carrier's own API when credentials are configured.
        // Tier 2: 17track, also covering first-party failures.
        let mut source = "17track";
        let result = match carriers::for_carrier(&parcel.resolved_carrier(), &config) {
            Some(provider) => match provider.track(&http, &parcel.tracking_number).await {
                Ok(info) => {
                    source = provider.name();
                    Ok(info)
                }
                Err(e) => {
                    print!("{} {}, trying 17track... ", "⚠".yellow(), e);
                    client.get_tracking_info(parcel).await
                }
            },
            None => client.get_tracking_info(parcel).await,
        };

        match result {
            Ok(info) => {
                parcel.tracking_info = Some(info);
                parcel.last_updated = Some(chrono::Utc::now());
                println!("{} via {}", "✓".green(), source);
            }
            Err(e) => {
                println!("{} ({})", "✗".red(), e);
            }
        }
    }

    save_parcels(&parcels)?;

    println!(
        "\n{} Updated {} parcel(s)",
        "✓".green().bold(),
        parcels.len()
    );

    Ok(())
}

fn config_cmd(track17_key: Option<String>) -> Result<()> {
    let mut config = load_config().unwrap_or_default();

    if track17_key.is_some() {
        config.track17_api_key = track17_key;
        storage::save_config(&config)?;
        println!("{} Credentials saved", "✓".green());
    }

    print_config_summary(&config);
    println!(
        "\nAmazon (TBA…) parcels are tracked via track.amazon.com with no
key; every other carrier uses 17track. Run `parceltracker setup`
for a guided walkthrough."
    );
    Ok(())
}

pub(crate) fn print_config_summary(config: &models::Config) {
    let set_or_dash = |v: &Option<String>| if v.is_some() { "set".green() } else { "—".dimmed() };
    println!("  Amazon (TBA…): {}", "built-in, no key needed".green());
    println!("  17track key:   {}", set_or_dash(&config.track17_api_key));
}

async fn select_for_waybar(identifier: &str) -> Result<()> {
    let parcels = load_parcels()?;

    let result = select_parcel_for_waybar(&parcels, identifier)?;
    println!("{}", result.green());

    Ok(())
}

async fn unselect_for_waybar() -> Result<()> {
    let result = unselect_parcel()?;
    println!("{}", result.green());
    Ok(())
}

fn parcel_carrier(parcel: &Parcel) -> Carrier {
    parcel.resolved_carrier()
}

async fn open_tracking(identifier: Option<&str>) -> Result<()> {
    let parcels = load_parcels()?;
    if parcels.is_empty() {
        println!("{}", "No parcels tracked.".yellow());
        return Ok(());
    }

    let target = if let Some(id) = identifier {
        if let Ok(pos) = id.parse::<usize>() {
            parcels.get(pos.saturating_sub(1))
        } else {
            parcels.iter().find(|p| p.tracking_number == id)
        }
    } else {
        resolve_waybar_parcel(&parcels)?
    };

    let Some(parcel) = target else {
        println!("{} Parcel not found", "✗".red().bold());
        return Ok(());
    };

    let carrier = parcel_carrier(parcel);
    let Some(url) = api::get_tracking_url(&carrier, &parcel.tracking_number) else {
        println!("{} No tracking URL for this carrier", "✗".red().bold());
        return Ok(());
    };

    let status = Command::new("xdg-open").arg(&url).status();
    match status {
        Ok(s) if s.success() => {
            println!(
                "{} Opened tracking page for {}",
                "✓".green().bold(),
                parcel.description.bold()
            );
        }
        Ok(_) | Err(_) => {
            println!(
                "{} Failed to open browser. URL: {}",
                "✗".red().bold(),
                url.cyan()
            );
        }
    }

    Ok(())
}

async fn cycle_waybar_selection() -> Result<()> {
    let parcels = load_parcels()?;
    if parcels.is_empty() {
        println!("{}", "No parcels tracked.".yellow());
        return Ok(());
    }

    let config = load_config()?;
    let next_idx = if let Some(sel) = config.waybar_selected {
        let current_idx = parcels
            .iter()
            .position(|p| p.tracking_number == sel.tracking)
            .unwrap_or(0);
        (current_idx + 1) % parcels.len()
    } else {
        0
    };

    let next = &parcels[next_idx];
    let result = select_parcel_for_waybar(&parcels, &next.tracking_number)?;
    println!("{}", result.green());
    Ok(())
}

async fn output_waybar() -> Result<()> {
    let parcels = load_parcels()?;
    let output = get_waybar_output(&parcels)?;
    println!("{}", output);
    Ok(())
}

async fn output_json() -> Result<()> {
    let parcels = load_parcels()?;
    let output = jsonout::get_json_output(&parcels)?;
    println!("{}", output);
    Ok(())
}
