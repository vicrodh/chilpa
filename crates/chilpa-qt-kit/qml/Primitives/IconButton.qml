import QtQuick
import "../Theme"

// A square button with only an icon, and always a tooltip (design/03).
Item {
    id: root

    required property string tip
    property string glyph
    property bool filled: false
    property string iconFamily
    property int size: Metrics.button28
    property int iconSize: size <= Metrics.button26 ? Metrics.iconSm : Metrics.iconMd
    property bool checked: false
    property bool busy: false
    // A circle on a filled background, for window controls and overlays.
    property bool round: false
    // "secondary", "primary", "accent", "solid" (the icon on the accent)
    // or "overlay" (on a cover: light, the accent when checked).
    property string tone: "secondary"
    // A favorite's heart: checked, it takes the theme's favorite colour.
    property bool heart: false

    signal clicked()

    implicitWidth: size
    implicitHeight: size

    Rectangle {
        anchors.fill: parent
        radius: root.round ? width / 2 : root.size <= Metrics.button26 ? Metrics.radiusItem : Metrics.radiusControl // derived: a circle
        color: root.tone === "solid" ? (area.containsMouse ? Theme.color.accentHover : Theme.color.accent)
             : root.tone === "overlay" ? Theme.color.overlayButton
             : root.checked ? Theme.color.stateSelected
             : area.containsMouse ? Theme.color.stateHover
             : root.round ? Theme.color.fillField : "transparent"
    }

    Icon {
        anchors.centerIn: parent
        visible: !root.busy
        glyph: root.glyph
        filled: root.filled
        iconFamily: root.iconFamily
        size: root.iconSize
        color: !root.enabled ? Theme.color.textTertiary
             : root.tone === "solid" ? Theme.color.textOnAccent
             : root.heart && root.checked ? Theme.color.favorite
             : root.tone === "overlay" ? (root.checked ? Theme.color.accent : Theme.color.overlayInk)
             : root.checked || root.tone === "accent" ? Theme.color.accent
             : root.tone === "primary" ? Theme.color.textPrimary : Theme.color.textSecondary
    }

    MouseArea {
        id: area
        cursorShape: root.busy ? Qt.BusyCursor : Qt.PointingHandCursor
        anchors.fill: parent
        hoverEnabled: true
        onClicked: if (!root.busy) root.clicked()
        onContainsMouseChanged: containsMouse && root.tip !== "" ? TipState.show(root.tip, root) : TipState.hide(root)
    }

    // A button that says something else after its click says it at once.
    // With nothing to say, as while what it opened is in sight, it says nothing.
    onTipChanged: if (area.containsMouse) root.tip !== "" ? TipState.show(root.tip, root) : TipState.hide(root)
}
