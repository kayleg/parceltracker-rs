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
  property string expandedId: ""
  property double nowMs: Date.now()

  readonly property int refreshIntervalSec: {
    var v = Number(setting("refreshIntervalSec", 60))
    if (!isFinite(v)) return 60
    return Math.min(3600, Math.max(15, Math.round(v)))
  }
  readonly property bool showLabel: setting("showLabel", true) === true

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
      var doc = Model.parseStatus(raw)
      if (doc === null) return
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
    command: ["parceltracker", "status", "--json"]
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: root.applyStatus(text) }
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
    contentHeight: Math.min(col.implicitHeight + padding * 2, Style.space(520))
    onOpenChanged: if (open) { root.expandedId = ""; root.refresh() }

    Column {
      id: col
      width: detail.contentWidth - detail.padding * 2
      spacing: Style.spacing.lg

      // ------------------------------------------------------------- hero
      Row {
        width: parent.width
        spacing: Style.spacing.sm
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
          text: !root.loaded ? "Loading…" : "No parcels tracked"
          color: Color.popups.text
          font.family: Style.font.family
          font.pixelSize: Style.font.subtitle
        }
      }
      Text {
        visible: root.view.hero === null && root.loaded
        text: "parceltracker add <tracking> [description]"
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
                onClicked: root.expandedId = rowRoot.expanded ? "" : rowRoot.modelData.id
              }

              Row {
                id: rowLine
                width: parent.width
                anchors.verticalCenter: parent.verticalCenter
                spacing: Style.spacing.sm

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
                  width: parent.width - Style.space(38) - eta.width - Style.spacing.sm * 2
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
      Row {
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
      }
    }
  }
}
