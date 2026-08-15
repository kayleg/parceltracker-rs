#!/usr/bin/env bash
# Installs the parceltracker CLI that the kayleg.parcel widget fronts.
# Launched by the widget's "Install CLI" chip in a floating terminal
# (Omarchy's plugin installer never runs code, so this stays manual and
# visible to the user).
set -u
REPO="https://github.com/kayleg/parceltracker-rs"
# Pinned commit + locked dependencies so the source reviewed for the
# marketplace listing cannot change underneath it. Bump REV in a new
# plugin commit whenever the CLI is intentionally updated.
REV="07920e240c16ed58421403b1d1bd98c9787ff751"

echo "Installing the parceltracker CLI from $REPO@${REV:0:12}"
echo
if ! command -v cargo >/dev/null 2>&1; then
  echo "The Rust toolchain is required, but 'cargo' was not found."
  echo "Install it with:"
  echo
  echo "  omarchy pkg add rustup && rustup default stable"
  echo
  echo "then click 'Install CLI' again."
elif cargo install --locked --git "$REPO" --rev "$REV"; then
  echo
  echo "✓ Installed. The widget will pick it up on its next refresh;"
  echo "  add a parcel with: parceltracker add <tracking> [description]"
else
  echo
  echo "✗ Install failed — see the output above."
fi
echo
read -rp "Press Enter to close... " _
