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

function stateLabel(state) {
  if (state === "out-for-delivery") return "Out for delivery"
  if (state === "in-transit") return "In transit"
  if (state === "pre-transit") return "Label created"
  if (state === "delivered") return "Delivered"
  if (state === "exception") return "Needs attention"
  return "No updates yet"
}

function carrierMonogram(carrier) {
  var map = {
    "UPS": "UPS",
    "FedEx": "FDX",
    "USPS": "USPS",
    "DHL": "DHL",
    "Canada Post": "CP",
    "OnTrac": "OT"
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
  return parcel.statusText || stateLabel(parcel.state)
}

function rowDetail(parcel, nowMs) {
  var event = (parcel.events && parcel.events.length > 0) ? parcel.events[0] : null
  var parts = []
  if (parcel.statusText) parts.push(parcel.statusText)
  else parts.push(stateLabel(parcel.state))
  if (event && event.location) parts.push(event.location)
  else if (parcel.location) parts.push(parcel.location)
  if (event && event.time) {
    var ago = relativeTime(event.time, nowMs)
    if (ago) parts.push(ago)
  }
  return parts.join(" · ")
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

if (typeof module !== "undefined" && module.exports)
  module.exports = {
    parseStatus: parseStatus,
    stateRole: stateRole,
    stateLabel: stateLabel,
    carrierMonogram: carrierMonogram,
    orderParcels: orderParcels,
    pickHero: pickHero,
    relativeTime: relativeTime,
    heroSubtitle: heroSubtitle,
    buildView: buildView,
    buildTooltip: buildTooltip,
    barLabel: barLabel
  }
