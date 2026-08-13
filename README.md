# Parcel Tracker

A Rust CLI with TUI for tracking parcels using the 17track API.

## Features

- **CLI Commands**: Add, remove, rename, list, and update parcels
- **Interactive TUI**: Navigate parcels with arrow keys, view details with Enter
- **Waybar Integration**: Display selected parcel status in waybar
- **Machine-readable status**: `parceltracker status --json` emits the full
  document (states, ETAs, checkpoint events, tracking URLs) — the contract
  behind the Omarchy shell plugin
- **Omarchy Quattro plugin**: `omarchy-plugin/` is a `kayleg.parcel` bar
  widget (state-colored badge, shipment list, checkpoint timelines). It is
  installed by the dotfiles quattro phase, which copies this directory to
  `~/.config/omarchy/plugins/kayleg.parcel/`. Logic tests:
  `cd omarchy-plugin && node tests/model.test.js`
- **Auto-detection**: Automatically detect carrier from tracking number patterns
- **2-hour timeout**: Selected delivered parcels are kept for 2 hours, then cleared

## Installation

```bash
cd /home/kayle/code/parceltracker-rs
cargo build --release
cp target/release/parceltracker ~/.local/bin/
```

## Configuration

Create `~/.local/share/parceltracker/config.json`:

```json
{
  "api_key": "your_17track_api_key_here",
  "waybar_selected": null
}
```

Get your API key from [17track.net](https://api.17track.net).

## Usage

### Commands

```bash
parceltracker                    # Launch TUI (default)
parceltracker tui               # Launch TUI
parceltracker add <tracking> [description]   # Add a parcel
parceltracker remove <position|tracking>     # Remove a parcel
parceltracker rename <position|tracking> <new_name>  # Rename a parcel
parceltracker list              # List all parcels (text table)
parceltracker update            # Update tracking info from API
parceltracker select <position|tracking>     # Select for waybar
parceltracker unselect          # Clear waybar selection
parceltracker waybar            # Output waybar JSON
parceltracker status --waybar   # Same as waybar (for waybar config)
```

### TUI Controls

- **↑/↓**: Navigate parcels
- **Enter**: View parcel details
- **1-9**: Select parcel for waybar display
- **u**: Unselect current selection
- **q/Esc**: Quit or close details view
- **PgUp/PgDn**: Scroll in details view
- **Home/End**: Jump to top/bottom in details

## Waybar Configuration

```json
"custom/parcel": {
  "exec": "parceltracker status --waybar",
  "interval": 60,
  "return-type": "json",
  "tooltip": true,
  "on-click": "parceltracker tui"
}
```

## Supported Carriers

- UPS: `1Z` prefix + 16 alphanumeric
- FedEx: 12, 15, 20, 22, or 34 digits
- DHL: 10 digits or JJ/JD/JM prefix
- USPS: 20-22 digits or 2-letter prefix + 9 digits + 2-letter suffix
- Canada Post: 16 digits or 2-letter prefix + 9 digits + "CA"
- OnTrac: C/D prefix + 14 digits, or 7-8 digits

## Data Storage

- Parcels: `~/.local/share/parceltracker/parcels.json`
- Config: `~/.local/share/parceltracker/config.json`
- Cache: `~/.local/share/parceltracker/cache.json` (reserved for future use)

## API Endpoints

- Register: `POST https://api.17track.net/track/v2.4/register`
- Get Info: `POST https://api.17track.net/track/v2.4/gettrackinfo`

Headers:
- `Content-Type: application/json`
- `17token: <api_key>`

## License

MIT
