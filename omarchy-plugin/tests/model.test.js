// Run with: node tests/model.test.js
const assert = require("node:assert")
const fs = require("node:fs")
const path = require("node:path")
const Model = require("../Model.js")

let failures = 0
function test(name, fn) {
  try {
    fn()
    console.log(`ok   ${name}`)
  } catch (err) {
    failures++
    console.error(`FAIL ${name}\n     ${err.message}`)
  }
}

const NOW = Date.parse("2026-08-13T16:00:00-04:00")
const rich = JSON.parse(
  fs.readFileSync(path.join(__dirname, "fixtures", "status.json"), "utf8")
)
const live = JSON.parse(
  fs.readFileSync(path.join(__dirname, "fixtures", "status-live.json"), "utf8")
)

test("parseStatus accepts v1 documents and rejects garbage", () => {
  assert.ok(Model.parseStatus(rich))
  assert.ok(Model.parseStatus(JSON.stringify(rich)))
  assert.strictEqual(Model.parseStatus({ version: 2, parcels: [] }), null)
  assert.strictEqual(Model.parseStatus({}), null)
})

test("ordering: soonest ETA first, no-ETA after, delivered last", () => {
  const view = Model.buildView(rich, NOW)
  assert.deepStrictEqual(
    view.rows.map(r => r.description),
    ["Espresso machine", "Keyboard", "Mystery box", "Socks"]
  )
  assert.strictEqual(view.count, 4)
  assert.strictEqual(view.liveCount, 3)
})

test("hero: explicit selection wins over soonest arrival", () => {
  const view = Model.buildView(rich, NOW)
  assert.strictEqual(view.hero.description, "Keyboard")
  assert.strictEqual(view.rows.filter(r => r.isHero).length, 1)
})

test("hero falls back to soonest arrival without a selection", () => {
  const doc = JSON.parse(JSON.stringify(rich))
  doc.selected = null
  const view = Model.buildView(doc, NOW)
  assert.strictEqual(view.hero.description, "Espresso machine")
})

test("state → color roles", () => {
  assert.strictEqual(Model.stateRole("exception"), "urgent")
  assert.strictEqual(Model.stateRole("out-for-delivery"), "accent")
  assert.strictEqual(Model.stateRole("delivered"), "muted")
  assert.strictEqual(Model.stateRole("in-transit"), "foreground")
  assert.strictEqual(Model.stateRole("pre-transit"), "foreground")
})

test("carrier monograms", () => {
  assert.strictEqual(Model.carrierMonogram("UPS"), "UPS")
  assert.strictEqual(Model.carrierMonogram("FedEx"), "FDX")
  assert.strictEqual(Model.carrierMonogram("Canada Post"), "CP")
  assert.strictEqual(Model.carrierMonogram("Amazon"), "AMZ")
  assert.strictEqual(Model.carrierMonogram("Pigeon Express"), "PKG")
})

test("relativeTime buckets", () => {
  assert.strictEqual(Model.relativeTime("2026-08-13T15:59:30-04:00", NOW), "just now")
  assert.strictEqual(Model.relativeTime("2026-08-13T15:15:00-04:00", NOW), "45m ago")
  assert.strictEqual(Model.relativeTime("2026-08-13T04:00:00-04:00", NOW), "12h ago")
  assert.strictEqual(Model.relativeTime("2026-08-10T16:00:00-04:00", NOW), "3d ago")
  assert.strictEqual(Model.relativeTime("not-a-date", NOW), "")
})

test("tooltip mirrors the waybar layout", () => {
  const tip = Model.buildTooltip(Model.buildView(rich, NOW))
  const lines = tip.split("\n")
  assert.ok(lines[0].startsWith("Keyboard · "))
  assert.ok(lines.includes("Other parcels:"))
  assert.strictEqual(lines.filter(l => l.startsWith("• ")).length, 3)
})

test("bar label carries hero description and subtitle", () => {
  const label = Model.barLabel(Model.buildView(rich, NOW))
  assert.ok(label.startsWith("Keyboard · "))
})

test("empty document renders an empty view", () => {
  const view = Model.buildView(Model.parseStatus({ version: 1, selected: null, parcels: [] }), NOW)
  assert.strictEqual(view.count, 0)
  assert.strictEqual(view.hero, null)
  assert.strictEqual(Model.buildTooltip(view), "No parcels tracked")
  assert.strictEqual(Model.barLabel(view), "")
})

test("live capture from this machine builds a view", () => {
  const view = Model.buildView(Model.parseStatus(live), NOW)
  assert.ok(view.count >= 1)
  assert.ok(view.hero !== null)
  assert.ok(view.rows[0].monogram.length >= 2)
})

test("rows carry the latest checkpoint location for the mini-map", () => {
  const view = Model.buildView(rich, NOW)
  const byDesc = Object.fromEntries(view.rows.map(r => [r.description, r.location]))
  assert.strictEqual(byDesc["Keyboard"], "Louisville, KY")
  assert.strictEqual(byDesc["Socks"], "Miami, FL")          // event beats parcel.location
  assert.strictEqual(byDesc["Mystery box"], "Fort Lauderdale, FL") // falls back to parcel.location
})

test("parseGeocode extracts the first Nominatim hit", () => {
  const raw = JSON.stringify([{ lat: "33.4918", lon: "-80.8556", display_name: "Orangeburg" }])
  assert.deepStrictEqual(Model.parseGeocode(raw), { lat: 33.4918, lon: -80.8556 })
  assert.strictEqual(Model.parseGeocode("[]"), null)
  assert.strictEqual(Model.parseGeocode("not json"), null)
  assert.strictEqual(Model.parseGeocode(JSON.stringify([{ lat: "x", lon: "y" }])), null)
})

test("mapLayout centers the point and covers the viewport", () => {
  // lat/lon 0,0 at zoom 0 in a 256×256 viewport is exactly tile (0,0)
  assert.deepStrictEqual(
    Model.mapLayout(0, 0, 0, 256, 256).tiles,
    [{ x: 0, y: 0, px: 0, py: 0 }]
  )

  const l = Model.mapLayout(33.4918, -80.8556, 10, 300, 140)
  const n = Math.pow(2, 10)
  assert.ok(l.tiles.length >= 2)
  for (const t of l.tiles) {
    assert.ok(t.x >= 0 && t.x < n && t.y >= 0 && t.y < n)
    assert.ok(t.px > -256 && t.px < l.width)
    assert.ok(t.py > -256 && t.py < l.height)
  }
  const xs = l.tiles.map(t => t.px)
  const ys = l.tiles.map(t => t.py)
  assert.ok(Math.min(...xs) <= 0 && Math.max(...xs) + 256 >= l.width)
  assert.ok(Math.min(...ys) <= 0 && Math.max(...ys) + 256 >= l.height)
})

test("mapLayout wraps tiles across the antimeridian", () => {
  const l = Model.mapLayout(0, 180, 2, 300, 140)
  const xs = l.tiles.map(t => t.x).sort()
  assert.ok(xs.includes(0) && xs.includes(3))
})

test("routeLocations: oldest first, dedupes stops, drops bare countries", () => {
  const route = Model.routeLocations({
    location: "ORANGEBURG, SC, US",
    events: [               // newest first, as jsonout emits them
      { location: "ORANGEBURG, SC, US" },
      { location: "INDEPENDENCE, KY, US" },
      { location: "INDEPENDENCE, KY, US" },
      { location: "US" },
      { location: "" }
    ]
  })
  assert.deepStrictEqual(route, ["INDEPENDENCE, KY, US", "ORANGEBURG, SC, US"])
  assert.deepStrictEqual(
    Model.routeLocations({ location: "Miami, FL", events: [] }),
    ["Miami, FL"]
  )
})

test("parseGeocodeMany keeps order and drops failed lookups", () => {
  const raw = JSON.stringify([
    [{ lat: "38.25", lon: "-85.75" }],
    null,
    [],
    [{ lat: "25.76", lon: "-80.19" }]
  ])
  assert.deepStrictEqual(Model.parseGeocodeMany(raw), [
    { lat: 38.25, lon: -85.75 },
    { lat: 25.76, lon: -80.19 }
  ])
  assert.deepStrictEqual(Model.parseGeocodeMany("garbage"), [])
})

test("mapScene fits a route with markers inside and arcs between them", () => {
  const pts = [
    { lat: 38.2527, lon: -85.7585 },  // Louisville
    { lat: 33.4918, lon: -80.8556 },  // Orangeburg
    { lat: 25.7617, lon: -80.1918 }   // Miami
  ]
  const s = Model.mapScene(pts, 300, 140, 11)
  assert.ok(s.zoom >= 1 && s.zoom <= 11)
  assert.strictEqual(s.markers.length, 3)
  assert.strictEqual(s.segments.length, 2)
  for (const m of s.markers) {
    assert.ok(m.px >= 0 && m.px <= 300 && m.py >= 0 && m.py <= 140)
  }
  for (const seg of s.segments) {
    // control point bulges away from the straight line
    assert.ok(seg.cy <= (seg.y1 + seg.y2) / 2)
  }
  const xs = s.tiles.map(t => t.px)
  const ys = s.tiles.map(t => t.py)
  assert.ok(Math.min(...xs) <= 0 && Math.max(...xs) + 256 >= 300)
  assert.ok(Math.min(...ys) <= 0 && Math.max(...ys) + 256 >= 140)
})

test("mapScene keeps Pacific-crossing routes contiguous", () => {
  const s = Model.mapScene(
    [{ lat: 35.68, lon: 139.69 }, { lat: 34.05, lon: -118.24 }],  // Tokyo → LA
    300, 140, 11
  )
  assert.strictEqual(s.markers.length, 2)
  assert.ok(Math.abs(s.markers[0].px - s.markers[1].px) <= 300)
  assert.strictEqual(s.segments.length, 1)
})

test("mapScene: single point centers, empty input is null", () => {
  const s = Model.mapScene([{ lat: 33.49, lon: -80.85 }], 300, 140, 11)
  assert.strictEqual(s.zoom, 10)
  assert.strictEqual(s.markers.length, 1)
  assert.ok(Math.abs(s.markers[0].px - 150) < 1 && Math.abs(s.markers[0].py - 70) < 1)
  assert.strictEqual(s.segments.length, 0)
  assert.strictEqual(Model.mapScene([], 300, 140, 11), null)
})

test("shellQuote survives bash -lc round trips", () => {
  assert.strictEqual(
    Model.shellQuote("https://ups.com/track?tracknum=1Z&x=y"),
    "'https://ups.com/track?tracknum=1Z&x=y'")
  assert.strictEqual(Model.shellQuote("it's here"), "'it'\\''s here'")
  assert.strictEqual(Model.shellQuote(""), "''")
})

process.exit(failures === 0 ? 0 : 1)
