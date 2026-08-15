# Parcel Tracker

A Rust CLI with TUI for tracking parcels. Amazon Logistics (TBA…)
parcels are tracked first-party via the public track.amazon.com
recipient endpoint — no key needed. Every other carrier is tracked
through the 17track aggregator API, which requires a key.

<img src="omarchy-plugin/preview.png" alt="Omarchy bar widget popup with checkpoint timeline and theme-colored route map" width="400"> <img src="omarchy-plugin/screenshots/popup-light.png" alt="The same popup in a light theme" width="400">

## Features

- **CLI Commands**: Add, remove, rename, list, and update parcels
- **Interactive TUI**: Navigate parcels with arrow keys, view details with Enter
- **Waybar Integration**: Display selected parcel status in waybar
- **Machine-readable status**: `parceltracker status --json` emits the full
  document (states, ETAs, checkpoint events, tracking URLs) — the contract
  behind the Omarchy shell plugin
- **Omarchy Quattro plugin**: `omarchy-plugin/` is a `kayleg.parcel` bar
  widget (state-colored badge, shipment list, checkpoint timelines),
  published for installation as
  [kayleg/omarchy-parcel](https://github.com/kayleg/omarchy-parcel)
  (`omarchy plugin add https://github.com/kayleg/omarchy-parcel.git`),
  which is split from this directory via
  `git subtree split --prefix=omarchy-plugin -b plugin-split` and pushed
  to that repo's `main`. Locally it is installed by the dotfiles quattro
  phase, which copies this directory to
  `~/.config/omarchy/plugins/kayleg.parcel/`. Logic tests:
  `cd omarchy-plugin && node tests/model.test.js`. Expanding a parcel row
  shows a mini-map of its checkpoint route — arcs connect each stop,
  ending at the current position (Nominatim geocoding + OSM tiles via
  `mapdata.sh`, cached in `~/.cache/parceltracker/map`; toggle with the
  `showMap` widget setting).
- **Auto-detection**: Automatically detect carrier from tracking number patterns
- **2-hour timeout**: Selected delivered parcels are kept for 2 hours, then cleared

## Installation

```bash
cd /home/kayle/code/parceltracker-rs
cargo build --release
cp target/release/parceltracker ~/.local/bin/
```

## Configuration

Amazon parcels need no configuration. For everything else, the
interactive wizard walks through getting a 17track key, validates it
live, and saves it (also reachable by pressing `s` in the TUI):

```bash
parceltracker setup
```

Or set the key directly (stored in
`~/.local/share/parceltracker/config.json`; run with no flags to see
what is configured):

```bash
# 17track key from https://api.17track.net (paid; new accounts get a
# one-time 200 free tracking numbers)
parceltracker config --track17-key <key>
```

## Usage

### Commands

```bash
parceltracker                    # Launch TUI (default)
parceltracker tui               # Launch TUI
parceltracker add <tracking> [description]   # Add a parcel
parceltracker remove <position|tracking>     # Remove a parcel
parceltracker rename <position|tracking> <new_name>  # Rename a parcel
parceltracker list              # List all parcels (text table)
parceltracker update            # Update tracking (first-party APIs, then 17track)
parceltracker setup             # Interactive wizard: get + validate API keys
parceltracker config [flags]    # Show or set API credentials
parceltracker select <position|tracking>     # Select the bar's lead parcel
parceltracker unselect          # Clear the bar selection
parceltracker waybar            # Output waybar JSON
parceltracker status --waybar   # Same as waybar (for waybar config)
```

### TUI Controls

- **↑/↓**: Navigate parcels
- **Enter**: View parcel details
- **1-9**: Select the parcel that leads the bar display
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

- Amazon Logistics: `TBA` prefix + 10-15 digits — tracked via the public
  track.amazon.com recipient endpoint, no key needed (unofficial, so it
  may need a parser update if Amazon changes the response)
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
