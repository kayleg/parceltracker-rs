// View-model logic for the kayleg.parcel bar widget, shared between the QML
// widget and the node test suite (node tests/model.test.js). Pure functions
// over the `parceltracker status --json` document (version 1). No Qt APIs.

function parseStatus(raw) {
  var doc = typeof raw === "string" ? JSON.parse(raw) : raw
  if (!doc || doc.version !== 1 || !Array.isArray(doc.parcels)) return null
  return doc
}

// Theme color role for a parcel state. The widget maps roles to the live
// Color/bar singletons so every theme swap recolors the badge and rows.
function stateRole(state) {
  if (state === "exception") return "urgent"
  if (state === "out-for-delivery") return "accent"
  if (state === "delivered") return "muted"
  return "foreground"
}

// Bar glyph for minimal mode: the state itself becomes the icon.
// Escapes because PUA glyphs are invisible in editors: truck (nf-fa-truck
// f0d1), check (nf-fa-check f00c), warning (nf-fa-warning f071), and the
// nf-md-package_variant_closed the widget already uses (f03d3).
function stateGlyph(state) {
  if (state === "out-for-delivery") return "\uf0d1"
  if (state === "delivered") return "\uf00c"
  if (state === "exception") return "\uf071"
  return "\u{f03d3}"
}

function stateLabel(state) {
  if (state === "out-for-delivery") return "Out for delivery"
  if (state === "in-transit") return "In transit"
  if (state === "pre-transit") return "Label created"
  if (state === "delivered") return "Delivered"
  if (state === "exception") return "Needs attention"
  return "No updates yet"
}

// Carrier status strings arrive as raw API codes ("InTransit",
// "Delivered_Other"); split the camel case and underscores for display.
function humanizeStatus(text) {
  if (!text) return ""
  var s = String(text).replace(/_/g, " ").replace(/([a-z0-9])([A-Z])/g, "$1 $2")
  return s.charAt(0).toUpperCase() + s.slice(1).toLowerCase()
}

function carrierMonogram(carrier) {
  var map = {
    "UPS": "UPS",
    "FedEx": "FDX",
    "USPS": "USPS",
    "DHL": "DHL",
    "Canada Post": "CP",
    "OnTrac": "OT",
    "Amazon": "AMZ"
  }
  return map[carrier] || "PKG"
}

function etaSortKey(parcel) {
  if (!parcel.eta) return Number.POSITIVE_INFINITY
  var t = Date.parse(parcel.eta)
  return isFinite(t) ? t : Number.POSITIVE_INFINITY
}

// Undelivered first, soonest ETA leading (unknown ETAs last); delivered
// parcels sink to the bottom in their stored order.
function orderParcels(parcels) {
  var live = []
  var done = []
  var list = parcels || []
  for (var i = 0; i < list.length; i++)
    (list[i].state === "delivered" ? done : live).push(list[i])
  live.sort(function (a, b) {
    var d = etaSortKey(a) - etaSortKey(b)
    if (d !== 0) return d < 0 ? -1 : 1
    return String(a.description || "").localeCompare(String(b.description || ""))
  })
  return live.concat(done)
}

// The parcel the bar leads with: the explicit selection when it still exists,
// otherwise the first of the ordered list.
function pickHero(doc) {
  if (!doc) return null
  var ordered = orderParcels(doc.parcels)
  if (doc.selected) {
    for (var i = 0; i < ordered.length; i++)
      if (ordered[i].trackingNumber === doc.selected) return ordered[i]
  }
  return ordered.length > 0 ? ordered[0] : null
}

function relativeTime(iso, nowMs) {
  var t = Date.parse(iso)
  if (!isFinite(t)) return ""
  var diff = Math.round((nowMs - t) / 1000)
  var suffix = diff >= 0 ? " ago" : " away"
  var s = Math.abs(diff)
  if (s < 90) return diff >= 0 ? "just now" : "moments away"
  if (s < 5400) return Math.round(s / 60) + "m" + suffix
  if (s < 129600) return Math.round(s / 3600) + "h" + suffix
  return Math.round(s / 86400) + "d" + suffix
}

function heroSubtitle(parcel) {
  if (!parcel) return ""
  if (parcel.state === "delivered") return "Delivered"
  if (parcel.etaLabel) return parcel.etaLabel
  return humanizeStatus(parcel.statusText) || stateLabel(parcel.state)
}

function rowDetail(parcel, nowMs) {
  var event = (parcel.events && parcel.events.length > 0) ? parcel.events[0] : null
  var parts = []
  if (parcel.statusText) parts.push(humanizeStatus(parcel.statusText))
  else parts.push(stateLabel(parcel.state))
  if (event && event.location) parts.push(event.location)
  else if (parcel.location) parts.push(parcel.location)
  if (event && event.time) {
    var ago = relativeTime(event.time, nowMs)
    if (ago) parts.push(ago)
  }
  return parts.join(" · ")
}

// ------------------------------------------------------------------- map
// Slippy-map support for the popup mini-map. The widget geocodes the
// parcel's checkpoint route (Nominatim, via mapdata.sh) and renders
// standard 256px OSM tiles with the route fitted into the viewport and
// arcs connecting consecutive checkpoints.

function parseGeocode(raw) {
  try {
    var arr = typeof raw === "string" ? JSON.parse(raw) : raw
    if (!Array.isArray(arr) || arr.length === 0) return null
    var lat = Number(arr[0].lat)
    var lon = Number(arr[0].lon)
    if (!isFinite(lat) || !isFinite(lon)) return null
    return { lat: lat, lon: lon }
  } catch (e) {
    return null
  }
}

// mapdata.sh geocode-many prints an array of per-location Nominatim
// results (null where the lookup failed). Failed entries drop out.
function parseGeocodeMany(raw) {
  try {
    var arr = typeof raw === "string" ? JSON.parse(raw) : raw
    if (!Array.isArray(arr)) return []
    var out = []
    for (var i = 0; i < arr.length; i++) {
      var geo = arr[i] === null ? null : parseGeocode(arr[i])
      if (geo !== null) out.push(geo)
    }
    return out
  } catch (e) {
    return []
  }
}

// Ordered checkpoint locations for the mini-map route, oldest first.
// Events arrive newest-first; consecutive repeats collapse, and bare
// country codes ("US") are too vague to geocode usefully.
function routeLocations(parcel) {
  var out = []
  var evs = parcel.events || []
  for (var i = evs.length - 1; i >= 0; i--) {
    var loc = evs[i] && evs[i].location
    if (!loc) continue
    if (loc.indexOf(",") < 0 && loc.length <= 3) continue
    if (out.length === 0 || out[out.length - 1] !== loc) out.push(loc)
  }
  if (out.length === 0 && parcel.location) out.push(parcel.location)
  return out.slice(-12)
}

// Tile grid covering a viewport whose top-left sits at originX/originY in
// world pixels. Tile x wraps across the antimeridian; rows beyond the
// poles are omitted (drawn as blank).
function tileGrid(originX, originY, width, height, n) {
  var tiles = []
  var ty0 = Math.floor(originY / 256)
  var ty1 = Math.ceil((originY + height) / 256) - 1
  var tx0 = Math.floor(originX / 256)
  var tx1 = Math.ceil((originX + width) / 256) - 1
  for (var ty = ty0; ty <= ty1; ty++) {
    if (ty < 0 || ty >= n) continue
    for (var tx = tx0; tx <= tx1; tx++) {
      tiles.push({
        x: ((tx % n) + n) % n,
        y: ty,
        px: tx * 256 - originX,
        py: ty * 256 - originY
      })
    }
  }
  return tiles
}

// Tiles covering a width×height viewport centered on a single lat/lon.
function mapLayout(lat, lon, zoom, width, height) {
  var n = Math.pow(2, zoom)
  var xf = ((lon + 180) / 360) * n
  var latRad = (lat * Math.PI) / 180
  var yf = ((1 - Math.log(Math.tan(latRad) + 1 / Math.cos(latRad)) / Math.PI) / 2) * n
  var originX = xf * 256 - width / 2
  var originY = yf * 256 - height / 2
  return {
    zoom: zoom,
    width: width,
    height: height,
    tiles: tileGrid(originX, originY, width, height, n)
  }
}

// Quadratic arcs between consecutive markers, bulging away from the
// straight line (upward where possible) for the route drawing.
function arcSegments(markers) {
  var segs = []
  for (var i = 0; i + 1 < markers.length; i++) {
    var a = markers[i]
    var b = markers[i + 1]
    var dx = b.px - a.px
    var dy = b.py - a.py
    var dist = Math.sqrt(dx * dx + dy * dy)
    if (dist < 2) continue
    var k = Math.min(40, Math.max(8, dist * 0.25))
    var ux = -dy / dist
    var uy = dx / dist
    if (uy > 0) { ux = -ux; uy = -uy }
    segs.push({
      x1: a.px, y1: a.py,
      cx: (a.px + b.px) / 2 + ux * k,
      cy: (a.py + b.py) / 2 + uy * k,
      x2: b.px, y2: b.py
    })
  }
  return segs
}

// Fit a whole route into the viewport: pick the deepest zoom that keeps
// every point inside the padded viewport, center on the route's bounding
// box, and return tiles plus per-point pixel markers and connecting arcs.
// Longitudes are unwrapped so Pacific-crossing routes stay contiguous.
function mapScene(points, width, height, maxZoom) {
  if (!points || points.length === 0) return null
  var lons = [points[0].lon]
  for (var i = 1; i < points.length; i++) {
    var l = points[i].lon
    while (l - lons[i - 1] > 180) l -= 360
    while (l - lons[i - 1] < -180) l += 360
    lons.push(l)
  }
  var base = []
  for (i = 0; i < points.length; i++) {
    var latRad = (points[i].lat * Math.PI) / 180
    base.push({
      x: (lons[i] + 180) / 360,
      y: (1 - Math.log(Math.tan(latRad) + 1 / Math.cos(latRad)) / Math.PI) / 2
    })
  }
  var pad = 24
  var zoom = Math.min(10, maxZoom)
  if (points.length > 1) {
    zoom = 1
    for (var z = maxZoom; z >= 1; z--) {
      var s = 256 * Math.pow(2, z)
      var minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity
      for (i = 0; i < base.length; i++) {
        minX = Math.min(minX, base[i].x * s); maxX = Math.max(maxX, base[i].x * s)
        minY = Math.min(minY, base[i].y * s); maxY = Math.max(maxY, base[i].y * s)
      }
      if (maxX - minX <= width - pad * 2 && maxY - minY <= height - pad * 2) {
        zoom = z
        break
      }
    }
  }
  var n = Math.pow(2, zoom)
  var scale = 256 * n
  var loX = Infinity, hiX = -Infinity, loY = Infinity, hiY = -Infinity
  for (i = 0; i < base.length; i++) {
    loX = Math.min(loX, base[i].x * scale); hiX = Math.max(hiX, base[i].x * scale)
    loY = Math.min(loY, base[i].y * scale); hiY = Math.max(hiY, base[i].y * scale)
  }
  var originX = (loX + hiX) / 2 - width / 2
  var originY = (loY + hiY) / 2 - height / 2
  var markers = []
  for (i = 0; i < base.length; i++) {
    var px = base[i].x * scale - originX
    var py = base[i].y * scale - originY
    var prev = markers[markers.length - 1]
    if (prev && Math.abs(prev.px - px) < 3 && Math.abs(prev.py - py) < 3) continue
    markers.push({ px: px, py: py })
  }
  return {
    zoom: zoom,
    width: width,
    height: height,
    tiles: tileGrid(originX, originY, width, height, n),
    markers: markers,
    segments: arcSegments(markers)
  }
}

function buildView(doc, nowMs) {
  if (!doc) return { rows: [], hero: null, count: 0, liveCount: 0 }
  var ordered = orderParcels(doc.parcels)
  var hero = pickHero(doc)
  var rows = []
  for (var i = 0; i < ordered.length; i++) {
    var p = ordered[i]
    rows.push({
      id: p.id,
      trackingNumber: p.trackingNumber,
      description: p.description || p.trackingNumber,
      carrier: p.carrier,
      monogram: carrierMonogram(p.carrier),
      state: p.state,
      role: stateRole(p.state),
      stateLabel: stateLabel(p.state),
      etaLabel: p.etaLabel || "",
      location: (p.events && p.events[0] && p.events[0].location) || p.location || "",
      route: routeLocations(p),
      detail: rowDetail(p, nowMs),
      trackingUrl: p.trackingUrl || "",
      events: p.events || [],
      isHero: hero !== null && p.trackingNumber === hero.trackingNumber
    })
  }
  var liveCount = 0
  for (var j = 0; j < ordered.length; j++)
    if (ordered[j].state !== "delivered") liveCount++
  return { rows: rows, hero: hero, count: ordered.length, liveCount: liveCount }
}

// Bar hover tooltip, mirroring the old waybar module: hero line first, the
// rest as bullets.
function buildTooltip(view) {
  if (!view || view.count === 0) return "No parcels tracked"
  var hero = view.hero
  var lines = [(hero.description || hero.trackingNumber) + " · " + heroSubtitle(hero)]
  var others = []
  for (var i = 0; i < view.rows.length; i++) {
    var row = view.rows[i]
    if (row.isHero) continue
    others.push("• " + row.description + " · " + (row.etaLabel || row.stateLabel))
  }
  if (others.length > 0) lines = lines.concat(["", "Other parcels:"], others)
  return lines.join("\n")
}

function barLabel(view) {
  if (!view || !view.hero) return ""
  return view.hero.description + " · " + heroSubtitle(view.hero)
}

// POSIX single-quote escaping for values interpolated into bar.run() commands
// (which execute via `bash -lc`).
function shellQuote(value) {
  return "'" + String(value).replace(/'/g, "'\\''") + "'"
}

if (typeof module !== "undefined" && module.exports)
  module.exports = {
    shellQuote: shellQuote,
    parseStatus: parseStatus,
    stateRole: stateRole,
    stateGlyph: stateGlyph,
    stateLabel: stateLabel,
    humanizeStatus: humanizeStatus,
    carrierMonogram: carrierMonogram,
    orderParcels: orderParcels,
    pickHero: pickHero,
    relativeTime: relativeTime,
    heroSubtitle: heroSubtitle,
    parseGeocode: parseGeocode,
    parseGeocodeMany: parseGeocodeMany,
    routeLocations: routeLocations,
    mapLayout: mapLayout,
    mapScene: mapScene,
    buildView: buildView,
    buildTooltip: buildTooltip,
    barLabel: barLabel
  }
