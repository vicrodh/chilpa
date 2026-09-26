import QtQuick
import "../Theme"

// A square button with only an icon, and always a tooltip (design/03).
Item {
    id: root

    required property string tip
    property string glyph
    property bool filled: false
    property int size: Metrics.button28
    property int iconSize: size <= Metrics.button26 ? Metrics.iconSm : Metrics.iconMd
    property bool checked: false
    // A circle on a filled background, for window controls and overlays.
    property bool round: false
    // "secondary", "primary", "accent", or "solid": the icon on the accent.
    property string tone: "secondary"

    signal clicked()

    implicitWidth: size
    implicitHeight: size

    Rectangle {
        anchors.fill: parent
        radius: root.round ? width / 2 : root.size <= Metrics.button26 ? Metrics.radiusItem : Metrics.radiusControl // derived: a circle
        color: root.tone === "solid" ? (area.containsMouse ? Theme.color.accentHover : Theme.color.accent)
             : root.checked ? Theme.color.stateSelected
             : area.containsMouse ? Theme.color.stateHover
             : root.round ? Theme.color.fillField : "transparent"
    }

    Icon {
        anchors.centerIn: parent
        glyph: root.glyph
        filled: root.filled
        size: root.iconSize
        color: !root.enabled ? Theme.color.textTertiary
             : root.tone === "solid" ? Theme.color.textOnAccent
             : root.checked || root.tone === "accent" ? Theme.color.accent
             : root.tone === "primary" ? Theme.color.textPrimary : Theme.color.textSecondary
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        onClicked: root.clicked()
        onContainsMouseChanged: containsMouse ? TipState.show(root.tip, root) : TipState.hide(root)
    }
}
