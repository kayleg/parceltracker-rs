#!/usr/bin/env bash
# Network fetcher for the kayleg.parcel popup mini-map. Everything is
# cached under ~/.cache/parceltracker/map so a location or tile is fetched
# from openstreetmap.org at most once, per the Nominatim/OSM usage
# policies (which also require the descriptive User-Agent below).
#
#   mapdata.sh geocode <location>          print Nominatim JSON for <location>
#   mapdata.sh geocode-many <loc> [loc..]  print a JSON array of results
#                                          (null where a lookup failed)
#   mapdata.sh tiles <zoom> <x:y> [x:y..]  ensure tiles are cached, print "ok"
#   mapdata.sh tiles-themed <zoom> <bg> <fg> <x:y> [x:y..]
#       ensure theme-recolored tiles (ImageMagick: grayscale mapped onto
#       fg..bg) are cached under themed/<bgfg-hex>/, print "ok"
set -u
UA="parceltracker-omarchy-widget/1.0 (personal desktop widget)"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/parceltracker/map"

# geo_file <location>: geocode with cache, print the cache file path on
# success. Nominatim allows at most 1 request/second, so uncached lookups
# after the first wait a beat.
fetched=0
geo_file() {
  mkdir -p "$CACHE/geo"
  key=$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | md5sum | cut -d' ' -f1)
  f="$CACHE/geo/$key.json"
  if [ ! -s "$f" ]; then
    [ "$fetched" -eq 1 ] && sleep 1
    fetched=1
    curl -fsS --max-time 10 -A "$UA" --get "https://nominatim.openstreetmap.org/search" \
      --data-urlencode "q=$1" --data-urlencode "format=json" --data-urlencode "limit=1" \
      -o "$f.tmp" && mv "$f.tmp" "$f" || { rm -f "$f.tmp"; return 1; }
  fi
  printf '%s' "$f"
}

# tile_file <z> <x> <y>: download a base OSM tile with cache, print its path.
tile_file() {
  f="$CACHE/tiles/$1/$2-$3.png"
  mkdir -p "$CACHE/tiles/$1"
  if [ ! -s "$f" ]; then
    curl -fsS --max-time 10 -A "$UA" -o "$f.tmp" \
      "https://tile.openstreetmap.org/$1/$2/$3.png" && mv "$f.tmp" "$f" || { rm -f "$f.tmp"; return 1; }
  fi
  printf '%s' "$f"
}

case "${1:-}" in
  geocode)
    loc="${2:-}"
    [ -n "$loc" ] || exit 1
    f=$(geo_file "$loc") || exit 1
    cat "$f"
    ;;
  geocode-many)
    shift
    out="["
    sep=""
    for loc in "$@"; do
      if f=$(geo_file "$loc") && [ -s "$f" ]; then
        out="$out$sep$(cat "$f")"
      else
        out="$out${sep}null"
      fi
      sep=","
    done
    printf '%s]\n' "$out"
    ;;
  tiles)
    z="${2:-}"
    [ -n "$z" ] || exit 1
    shift 2
    for t in "$@"; do
      tile_file "$z" "${t%%:*}" "${t##*:}" >/dev/null || true
    done
    echo ok
    ;;
  tiles-themed)
    z="${2:-}"; bg="${3:-}"; fg="${4:-}"
    { [ -n "$z" ] && [ -n "$bg" ] && [ -n "$fg" ]; } || exit 1
    shift 4
    key=$(printf '%s%s' "$bg" "$fg" | tr -d '#')
    mkdir -p "$CACHE/themed/$key/$z"
    for t in "$@"; do
      x="${t%%:*}"; y="${t##*:}"
      out="$CACHE/themed/$key/$z/$x-$y.png"
      [ -s "$out" ] && continue
      base=$(tile_file "$z" "$x" "$y") || continue
      magick "$base" -colorspace Gray +level-colors "$fg","$bg" "$out" || rm -f "$out"
    done
    echo ok
    ;;
  *)
    exit 1
    ;;
esac
