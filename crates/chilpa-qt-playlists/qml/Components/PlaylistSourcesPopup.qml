import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Whose playlists the manager lists, one switch per source as the Library's
// "Which sources count". Every switch is on while nothing narrows the list;
// turning one off keeps the rest, and the last one stays on.
Popup {
    id: popup
    PopupInput { surface: popup }
    // The ids on offer ("local" for the library's own) and how each is named
    // and drawn.
    property var sources: []
    property var labelOf: function(id) { return id }
    property var iconOf: function(id) { return "" }
    // The ids that count, comma separated; empty is every one.
    property string chosen: ""
    signal picked(string chosen)
    readonly property var on: chosen === "" ? sources : chosen.split(",")

    function flip(id, to) {
        const next = sources.filter(one => one === id ? to : on.indexOf(one) >= 0)
        if (next.length === 0) return
        picked(next.length === sources.length ? "" : next.join(","))
    }

    modal: true
    dim: false
    focus: true
    width: Metrics.sourcesScopeWidth
    padding: Metrics.gap6
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnReleaseOutside

    background: Item {
        Hold {}
        RectangularShadow { anchors.fill: panel; radius: panel.radius; offset.y: Metrics.shadowElevatedY; blur: Metrics.shadowElevatedBlur; color: Theme.color.shadowElevated }
        RectangularShadow { anchors.fill: panel; radius: panel.radius; offset.y: Metrics.shadowKeyY; blur: Metrics.shadowKeyBlur; color: Theme.color.shadowElevatedKey }
        Rectangle { id: panel; anchors.fill: parent; radius: Metrics.radiusMenu; color: Theme.color.surfaceRaised; Edge {} }
    }

    contentItem: Column {
        spacing: Metrics.hairline * 2 // derived: the design's 2 px between rows
        Text {
            leftPadding: Metrics.gap8
            topPadding: Metrics.gap4
            bottomPadding: Metrics.gap6
            text: I18n.tr("Which sources count", I18n.rev)
            font.pointSize: Type.label
            font.weight: Type.semibold
            color: Theme.color.textTertiary
        }
        Repeater {
            model: popup.sources
            Rectangle {
                id: entry
                required property string modelData
                readonly property bool lit: popup.on.indexOf(modelData) >= 0
                objectName: "playlistSourceRow"
                width: popup.availableWidth
                height: Metrics.button40
                radius: Metrics.radiusControl
                color: rowHover.hovered ? Theme.color.stateHover : "transparent"
                HoverHandler { id: rowHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: popup.flip(entry.modelData, !entry.lit) }
                SourceIcon {
                    id: glyph
                    x: Metrics.gap8
                    anchors.verticalCenter: parent.verticalCenter
                    width: Metrics.iconNav
                    height: Metrics.iconNav
                    sourceInfo: entry.modelData === "local" ? ({ local: true }) : ({ icon: popup.iconOf(entry.modelData) })
                }
                Text {
                    anchors.left: glyph.right
                    anchors.leftMargin: Metrics.gap10
                    anchors.right: toggle.left
                    anchors.rightMargin: Metrics.gap10
                    anchors.verticalCenter: parent.verticalCenter
                    text: popup.labelOf(entry.modelData)
                    elide: Text.ElideRight
                    font.pointSize: Type.body
                    color: Theme.color.textPrimary
                }
                Toggle {
                    id: toggle
                    objectName: "playlistSource-" + entry.modelData
                    compact: true
                    anchors.right: parent.right
                    anchors.rightMargin: Metrics.gap8
                    anchors.verticalCenter: parent.verticalCenter
                    checked: entry.lit
                    onToggled: (on) => popup.flip(entry.modelData, on)
                }
            }
        }
    }
}
