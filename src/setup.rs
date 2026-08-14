// Interactive first-run wizard (`parceltracker setup`): explains what
// works out of the box, walks through getting a 17track key for
// everything else, live-validates the entry before saving, and writes
// the result to config.json. Reached from the CLI or by pressing `s` in
// the TUI (which suspends and resumes around it).

use anyhow::Result;
use colored::Colorize;
use std::io::{self, Write};

use crate::storage::{load_config, save_config};

fn prompt(label: &str) -> Result<String> {
    print!("  {}: ", label);
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

fn confirm(label: &str) -> Result<bool> {
    let answer = prompt(&format!("{} [y/N]", label))?;
    Ok(matches!(answer.to_lowercase().as_str(), "y" | "yes"))
}

pub async fn run() -> Result<()> {
    println!("{}", "Parcel Tracker setup".bold());
    println!(
        "\nAmazon (TBA…) parcels work out of the box — no key needed. Every\n\
         other carrier (FedEx, UPS, USPS, DHL, …) is tracked through the\n\
         17track aggregator, which requires an API key (paid; new accounts\n\
         get a one-time 200 free tracking numbers). Press Enter to skip."
    );

    let mut config = load_config().unwrap_or_default();

    println!("\n{}", "── 17track".bold());
    println!(
        "  Currently: {}",
        if config.track17_api_key.is_some() {
            "configured (Enter keeps the existing key)".green()
        } else {
            "not configured".dimmed()
        }
    );
    println!("  1. Register at https://api.17track.net");
    println!("  2. Copy the access key from the API admin console");

    let key = prompt("API key (Enter to skip)")?;
    if !key.is_empty() {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()?;
        print!("  Validating… ");
        io::stdout().flush()?;
        match crate::api::validate_key(&http, &key).await {
            Ok(()) => {
                println!("{}", "✓ key works".green());
                config.track17_api_key = Some(key);
            }
            Err(e) => {
                println!("{} {}", "✗".red(), e);
                if confirm("Save anyway?")? {
                    config.track17_api_key = Some(key);
                } else {
                    println!("  {}", "discarded".dimmed());
                }
            }
        }
    }

    save_config(&config)?;
    println!("\n{} Configuration saved", "✓".green().bold());
    crate::print_config_summary(&config);
    println!(
        "\nNext: {} — the update output shows which provider served each parcel.",
        "parceltracker update".cyan()
    );
    Ok(())
}
