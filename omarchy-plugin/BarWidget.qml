import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

// Parcel tracking in the bar, backed by the parceltracker CLI. The badge
// carries the lead parcel (explicit selection, else soonest arrival) and
// colors by its state; the popup lists every shipment with an expandable
// checkpoint timeline. Left = popup, right = carrier tracking page,
// middle = refresh from the carrier API.
//
// All colors come from the live theme singletons (bar foreground, Color
// accent/urgent/muted, Color.popups.*), so theme swaps restyle everything.
BarWidget {
  id: root
  moduleName: "kayleg.parcel"

  property var view: Model.buildView(null, Date.now())
  property bool loaded: false
  property bool cliMissing: false
  property string expandedId: ""
  property double nowMs: Date.now()

  readonly property int refreshIntervalSec: {
    var v = Number(setting("refreshIntervalSec", 60))
    if (!isFinite(v)) return 60
    return Math.min(3600, Math.max(15, Math.round(v)))
  }
  readonly property bool showLabel: setting("showLabel", true) === true
  readonly property bool minimal: setting("minimal", false) === true
  readonly property bool showMap: setting("showMap", true) === true
  readonly property bool themedMap: setting("themedMap", true) === true

  // Popup mini-map state: the expanded row's latest checkpoint location is
  // geocoded and rendered from cached OSM tiles (fetched by mapdata.sh).
  readonly property int mapZoom: 11    // deepest zoom the route fit may pick
  property var mapView: null
  property bool mapReady: false
  readonly property string mapCacheDir: {
    var xdg = Quickshell.env("XDG_CACHE_HOME")
    return (xdg && xdg.length > 0 ? xdg : Quickshell.env("HOME") + "/.cache") + "/parceltracker/map"
  }
  readonly property string mapScript: Qt.resolvedUrl("mapdata.sh").toString().replace(/^file:\/\//, "")

  function clearMap() {
    mapReady = false
    mapView = null
  }

  function requestMap(route) {
    clearMap()
    if (!showMap || !route || route.length === 0) return
    geocodeProc.command = ["bash", mapScript, "geocode-many"].concat(route)
    geocodeProc.running = true
  }

  function applyGeocode(raw) {
    if (expandedId === "") return
    var scene = Model.mapScene(Model.parseGeocodeMany(raw),
                               col.width - Style.space(46), Style.space(140), mapZoom)
    if (scene === null) return
    mapView = scene
    fetchMapTiles()
  }

  function colorHex(c) {
    function h(v) {
      var s = Math.round(Math.min(1, Math.max(0, v)) * 255).toString(16)
      return s.length === 1 ? "0" + s : s
    }
    return "#" + h(c.r) + h(c.g) + h(c.b)
  }

  // Fetch (and, for themed mode, recolor) the current scene's tiles.
  // Qt 6's Canvas putImageData is a silent no-op, so theming happens on
  // disk: mapdata.sh maps each tile's grayscale onto ink..background with
  // ImageMagick, cached per color pair. mapTileDir is the single source
  // of truth for where tilePath() reads from.
  property string mapTileDir: ""
  function fetchMapTiles() {
    if (mapView === null) return
    mapReady = false
    var args = ["bash", mapScript]
    if (themedMap) {
      var bg = Color.popups.background
      var fg = Color.popups.text
      var ink = Qt.rgba(fg.r + (bg.r - fg.r) * 0.15,
                        fg.g + (bg.g - fg.g) * 0.15,
                        fg.b + (bg.b - fg.b) * 0.15, 1)
      var bgHex = colorHex(bg)
      var inkHex = colorHex(ink)
      mapTileDir = mapCacheDir + "/themed/"
        + (bgHex + inkHex).replace(/#/g, "") + "/" + mapView.zoom
      args = args.concat(["tiles-themed", String(mapView.zoom), bgHex, inkHex])
    } else {
      mapTileDir = mapCacheDir + "/tiles/" + mapView.zoom
      args = args.concat(["tiles", String(mapView.zoom)])
    }
    for (var i = 0; i < mapView.tiles.length; i++)
      args.push(mapView.tiles[i].x + ":" + mapView.tiles[i].y)
    tileProc.command = args
    tileProc.running = true
  }

  function tilePath(tile) {
    return "file://" + mapTileDir + "/" + tile.x + "-" + tile.y + ".png"
  }

  function roleColor(role, fallback) {
    if (role === "urgent") return Color.urgent
    if (role === "accent") return Color.accent
    if (role === "muted") return Color.muted
    return fallback
  }

  readonly property color heroColor: {
    var hero = view ? view.hero : null
    var base = root.bar ? root.bar.barForeground : Color.foreground
    return hero ? roleColor(Model.stateRole(hero.state), base) : base
  }

  function refresh() {
    if (!statusProc.running) statusProc.running = true
  }

  function applyStatus(raw) {
    try {
      // Empty output means the CLI is not on PATH (bash -lc exits 127
      // with nothing on stdout); surface an install hint in the popup.
      if (!raw || raw.trim().length === 0) {
        if (!loaded) cliMissing = true
        return
      }
      var doc = Model.parseStatus(raw)
      if (doc === null) return
      cliMissing = false
      nowMs = Date.now()
      view = Model.buildView(doc, nowMs)
      loaded = true
    } catch (e) { /* keep last good view */ }
  }

  function openTracking(url) {
    if (!root.bar || !url) return
    root.bar.run("xdg-open " + Model.shellQuote(url))
  }

  function updateFromCarrier() {
    if (root.bar) root.bar.run("parceltracker update")
    updateSettle.restart()
  }

  Process {
    id: statusProc
    // Login shell so cargo-installed binaries (~/.cargo/bin, ~/.local/bin)
    // are found even when the shell process' own PATH lacks them.
    command: ["bash", "-lc", "parceltracker status --json"]
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: root.applyStatus(text) }
  }

  Process {
    id: geocodeProc
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: root.applyGeocode(text) }
  }

  Process {
    id: tileProc
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: root.mapReady = text.indexOf("ok") === 0 }
  }

  Timer {
    interval: root.refreshIntervalSec * 1000
    running: true
    repeat: true
    triggeredOnStart: true
    onTriggered: root.refresh()
  }

  // Carrier updates take a few seconds server-side; re-read once they settle.
  Timer {
    id: updateSettle
    interval: 8000
    repeat: false
    onTriggered: root.refresh()
  }

  // Minimal theme-native text button; deliberately self-contained rather
  // than depending on panel form controls whose contracts may shift.
  component ActionChip: Rectangle {
    id: chip
    property string label: ""
    property bool interactive: true
    signal activated()
    opacity: interactive ? 1 : 0.5
    implicitWidth: chipText.implicitWidth + Style.space(16)
    implicitHeight: chipText.implicitHeight + Style.space(10)
    radius: Style.cornerRadius > 0 ? Math.min(Style.cornerRadius, height / 2) : 4
    color: chipMouse.containsMouse && chip.interactive
      ? Qt.rgba(Color.popups.text.r, Color.popups.text.g, Color.popups.text.b, 0.12)
      : "transparent"
    border.width: 1
    border.color: Qt.rgba(Color.popups.text.r, Color.popups.text.g, Color.popups.text.b, 0.35)
    Text {
      id: chipText
      anchors.centerIn: parent
      text: chip.label
      color: Color.popups.text
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
    MouseArea {
      id: chipMouse
      anchors.fill: parent
      enabled: chip.interactive
      hoverEnabled: true
      cursorShape: chip.interactive ? Qt.PointingHandCursor : Qt.ArrowCursor
      onClicked: chip.activated()
    }
  }

  visible: true
  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    foreground: root.heroColor
    useActiveColor: false
    fontSize: Style.font.body
    text: {
      // Minimal mode: a lone state-colored status icon, no name or text.
      if (root.minimal)
        return Model.stateGlyph(root.view && root.view.hero ? root.view.hero.state : "")
      var glyph = "󰏓"   // 󰏓 nf-md-package_variant_closed
      if (root.vertical || !root.showLabel || !root.view || !root.view.hero) return glyph
      return glyph + "  " + Model.barLabel(root.view)
    }
    tooltipText: Model.buildTooltip(root.view)
    onPressed: function (mousebutton) {
      if (mousebutton === Qt.RightButton) {
        if (root.bar) root.bar.run("parceltracker open")
      } else if (mousebutton === Qt.MiddleButton) {
        root.updateFromCarrier()
      } else {
        detail.open = !detail.open
      }
    }
  }

  PopupCard {
    id: detail
    anchorItem: button
    bar: root.bar
    owner: root
    contentWidth: Style.space(340)
    contentHeight: Math.min(col.implicitHeight + padding * 2, Style.space(620))
    onOpenChanged: if (open) { root.expandedId = ""; root.clearMap(); root.refresh() }

    Column {
      id: col
      width: detail.contentWidth - detail.padding * 2
      spacing: Style.spacing.lg

      // ------------------------------------------------------------- hero
      Row {
        width: parent.width
        spacing: Style.spacing.xxl
        visible: root.view.hero !== null

        Text {
          anchors.verticalCenter: parent.verticalCenter
          text: "󰏓"
          color: root.heroColor
          font.family: Style.font.family
          font.pixelSize: Style.font.heading
        }
        Column {
          anchors.verticalCenter: parent.verticalCenter
          spacing: 2
          Text {
            text: root.view.hero ? (root.view.hero.description || root.view.hero.trackingNumber) : ""
            color: Color.popups.text
            font.family: Style.font.family
            font.pixelSize: Style.font.subtitle
            font.bold: true
          }
          Text {
            text: root.view.hero ? Model.heroSubtitle(root.view.hero) : ""
            color: root.heroColor
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
        }
      }

      Row {
        width: parent.width
        spacing: Style.spacing.sm
        visible: root.view.hero === null
        Text {
          text: root.cliMissing ? "parceltracker CLI not installed"
            : (!root.loaded ? "Loading…" : "No parcels tracked")
          color: Color.popups.text
          font.family: Style.font.family
          font.pixelSize: Style.font.subtitle
        }
      }
      Text {
        visible: root.view.hero === null && (root.loaded || root.cliMissing)
        text: root.cliMissing
          ? "cargo install --git https://github.com/kayleg/parceltracker-rs"
          : "parceltracker add <tracking> [description]"
        color: Qt.rgba(Color.popups.text.r, Color.popups.text.g, Color.popups.text.b, 0.6)
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }

      PanelSeparator { visible: root.view.count > 0; foreground: Color.popups.text }

      // ------------------------------------------------------- parcel rows
      Column {
        width: parent.width
        spacing: Style.spacing.sm
        Repeater {
          model: root.view.rows
          Column {
            id: rowRoot
            required property var modelData
            readonly property bool expanded: root.expandedId === modelData.id
            readonly property color rowColor: root.roleColor(modelData.role, Color.popups.text)
            width: parent.width
            spacing: Style.spacing.xs

            Item {
              width: parent.width
              height: rowLine.implicitHeight + Style.space(6)

              MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                  if (rowRoot.expanded) {
                    root.expandedId = ""
                    root.clearMap()
                  } else {
                    root.expandedId = rowRoot.modelData.id
                    root.requestMap(rowRoot.modelData.route)
                  }
                }
              }

              Row {
                id: rowLine
                width: parent.width
                anchors.verticalCenter: parent.verticalCenter
                spacing: Style.spacing.xxl

                Rectangle {
                  anchors.verticalCenter: parent.verticalCenter
                  width: Style.space(38)
                  height: Style.space(18)
                  radius: Style.cornerRadius > 0 ? Math.min(Style.cornerRadius, height / 2) : 4
                  color: "transparent"
                  border.width: 1
                  border.color: rowRoot.rowColor
                  Text {
                    anchors.centerIn: parent
                    text: rowRoot.modelData.monogram
                    color: rowRoot.rowColor
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                    font.bold: true
                  }
                }

                Column {
                  width: parent.width - Style.space(38) - eta.width - Style.spacing.xxl * 2
                  anchors.verticalCenter: parent.verticalCenter
                  spacing: 1
                  Text {
                    width: parent.width
                    text: rowRoot.modelData.description
                    color: Color.popups.text
                    elide: Text.ElideRight
                    font.family: Style.font.family
                    font.pixelSize: Style.font.bodySmall
                    font.bold: rowRoot.modelData.isHero
                  }
                  Text {
                    width: parent.width
                    text: rowRoot.modelData.detail
                    color: Qt.rgba(Color.popups.text.r, Color.popups.text.g, Color.popups.text.b, 0.6)
                    elide: Text.ElideRight
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                  }
                }

                Text {
                  id: eta
                  anchors.verticalCenter: parent.verticalCenter
                  text: rowRoot.modelData.etaLabel !== ""
                    ? rowRoot.modelData.etaLabel : rowRoot.modelData.stateLabel
                  color: rowRoot.rowColor
                  font.family: Style.font.family
                  font.pixelSize: Style.font.caption
                }
              }
            }

            // --------------------------------------------- event timeline
            Column {
              visible: rowRoot.expanded
              width: parent.width
              spacing: Style.spacing.xs
              leftPadding: Style.space(46)

              // Mini-map of the latest checkpoint, once its tiles are cached.
              Rectangle {
                visible: rowRoot.expanded && root.mapReady && root.mapView !== null
                width: parent.width - Style.space(46)
                height: root.mapView ? root.mapView.height : 0
                color: "transparent"
                border.width: 1
                border.color: Qt.rgba(Color.popups.text.r, Color.popups.text.g, Color.popups.text.b, 0.25)

                Item {
                  anchors.fill: parent
                  anchors.margins: 1
                  clip: true

                  // Tiles, route arcs, and checkpoint dots in one canvas.
                  // Tiles arrive already theme-recolored from mapdata.sh;
                  // a theme swap refetches them for the new color pair.
                  Canvas {
                    anchors.fill: parent
                    property var scene: root.mapReady ? root.mapView : null
                    property color themeBg: Color.popups.background
                    property color themeFg: Color.popups.text
                    onSceneChanged: requestPaint()
                    onThemeBgChanged: root.fetchMapTiles()
                    onThemeFgChanged: root.fetchMapTiles()
                    onImageLoaded: requestPaint()
                    onPaint: {
                      var ctx = getContext("2d")
                      ctx.reset()
                      if (!scene) return
                      var i
                      for (i = 0; i < scene.tiles.length; i++) {
                        var url = root.tilePath(scene.tiles[i])
                        if (isImageLoaded(url))
                          ctx.drawImage(url, scene.tiles[i].px, scene.tiles[i].py, 256, 256)
                        else
                          loadImage(url)
                      }
                      if (!root.themedMap) {
                        // raw OSM colors: just soften toward the popup bg
                        ctx.fillStyle = Qt.rgba(themeBg.r, themeBg.g, themeBg.b, 0.18)
                        ctx.fillRect(0, 0, width, height)
                      }
                      var c = rowRoot.rowColor
                      ctx.strokeStyle = Qt.rgba(c.r, c.g, c.b, 0.85)
                      ctx.fillStyle = Qt.rgba(c.r, c.g, c.b, 1)
                      ctx.lineWidth = 2
                      var segs = scene.segments
                      for (i = 0; i < segs.length; i++) {
                        ctx.beginPath()
                        ctx.moveTo(segs[i].x1, segs[i].y1)
                        ctx.quadraticCurveTo(segs[i].cx, segs[i].cy, segs[i].x2, segs[i].y2)
                        ctx.stroke()
                      }
                      for (i = 0; i < scene.markers.length - 1; i++) {
                        ctx.beginPath()
                        ctx.arc(scene.markers[i].px, scene.markers[i].py, 3, 0, Math.PI * 2)
                        ctx.fill()
                      }
                    }
                  }

                  // Current position: the route's newest point.
                  Rectangle {
                    visible: root.mapReady && root.mapView !== null && root.mapView.markers.length > 0
                    x: visible ? root.mapView.markers[root.mapView.markers.length - 1].px - width / 2 : 0
                    y: visible ? root.mapView.markers[root.mapView.markers.length - 1].py - height / 2 : 0
                    width: Style.space(12)
                    height: width
                    radius: width / 2
                    color: rowRoot.rowColor
                    border.width: 2
                    border.color: "#ffffff"
                  }

                  Text {
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: 2
                    text: "© OpenStreetMap"
                    color: root.themedMap ? Color.popups.text : "#000000"
                    opacity: 0.55
                    font.family: Style.font.family
                    font.pixelSize: Math.max(8, Style.font.caption - 2)
                  }
                }
              }

              Repeater {
                model: rowRoot.expanded ? rowRoot.modelData.events.slice(0, 5) : []
                Column {
                  required property var modelData
                  width: parent.width - Style.space(46)
                  spacing: 0
                  Text {
                    width: parent.width
                    text: Model.relativeTime(modelData.time, root.nowMs)
                      + (modelData.location ? " · " + modelData.location : "")
                    color: Qt.rgba(Color.popups.text.r, Color.popups.text.g, Color.popups.text.b, 0.55)
                    elide: Text.ElideRight
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                  }
                  Text {
                    width: parent.width
                    text: modelData.description
                    color: Color.popups.text
                    wrapMode: Text.WordWrap
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                  }
                }
              }

              Text {
                visible: rowRoot.expanded && rowRoot.modelData.events.length === 0
                text: "No checkpoints yet"
                color: Qt.rgba(Color.popups.text.r, Color.popups.text.g, Color.popups.text.b, 0.55)
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
              }

              ActionChip {
                visible: rowRoot.expanded && rowRoot.modelData.trackingUrl !== ""
                label: "Open " + rowRoot.modelData.carrier + " tracking"
                onActivated: { root.openTracking(rowRoot.modelData.trackingUrl); detail.open = false }
              }
            }
          }
        }
      }

      PanelSeparator { visible: root.view.count > 0; foreground: Color.popups.text }

      // ------------------------------------------------------------ footer
      Flow {
        width: parent.width
        spacing: Style.spacing.sm
        ActionChip {
          label: updateSettle.running ? "Refreshing…" : "Refresh from carrier"
          interactive: !updateSettle.running
          onActivated: root.updateFromCarrier()
        }
        ActionChip {
          visible: root.view.hero !== null && root.view.hero.trackingUrl !== ""
          label: "Open tracking page"
          onActivated: { root.openTracking(root.view.hero.trackingUrl); detail.open = false }
        }
        ActionChip {
          label: "Open TUI"
          onActivated: {
            if (root.bar) root.bar.run("omarchy launch or focus tui parceltracker")
            detail.open = false
          }
        }
        // Persist the toggle through `omarchy bar set`, which patches this
        // widget's entry in shell.json; the settings hot-reload flips the
        // bar text live, so the chip label follows automatically.
        ActionChip {
          label: root.minimal ? "Show bar label" : "Minimal bar icon"
          onActivated: {
            if (root.bar)
              root.bar.run("omarchy bar set " + root.moduleName + " minimal "
                           + (root.minimal ? "false" : "true") + " --json")
          }
        }
      }
    }
  }
}
