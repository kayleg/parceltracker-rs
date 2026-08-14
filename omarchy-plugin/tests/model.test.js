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

test("shellQuote survives bash -lc round trips", () => {
  assert.strictEqual(
    Model.shellQuote("https://ups.com/track?tracknum=1Z&x=y"),
    "'https://ups.com/track?tracknum=1Z&x=y'")
  assert.strictEqual(Model.shellQuote("it's here"), "'it'\\''s here'")
  assert.strictEqual(Model.shellQuote(""), "''")
})

process.exit(failures === 0 ? 0 : 1)
