import QtQuick
import "../Theme"

// Small panel actions keep a full keyboard and pointer target.
Rectangle {
    id: button
    property string text: ""
    property string glyph: ""
    property string tip: text
    property bool primary: false
    property bool quiet: false
    signal clicked()
    implicitHeight: Metrics.hitMin
    implicitWidth: label.implicitWidth + (glyph ? Metrics.iconSm : 0) + (glyph && text ? Metrics.gap6 : 0) + Metrics.gap10 * 2 // derived: horizontal insets
    radius: Metrics.radiusRow
    opacity: enabled ? 1 : Metrics.disabledOpacity
    color: primary ? (hover.hovered ? Theme.color.accentHover : Theme.color.accent)
        : hover.hovered ? Theme.color.stateHover : "transparent"
    border.width: quiet || primary ? 0 : Metrics.hairline
    border.color: Theme.color.strokeHairline
    activeFocusOnTab: enabled
    Accessible.role: Accessible.Button
    Accessible.name: tip
    Accessible.onPressAction: clicked()
    Keys.onReturnPressed: clicked()
    Keys.onEnterPressed: clicked()
    Keys.onSpacePressed: clicked()
    Row {
        anchors.centerIn: parent
        spacing: Metrics.gap6
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: button.glyph !== ""
            glyph: button.glyph
            size: Metrics.iconSm
            color: button.primary ? Theme.color.textOnAccent : Theme.color.textSecondary
        }
        Text {
            id: label
            anchors.verticalCenter: parent.verticalCenter
            text: button.text
            font.pointSize: Type.meta
            font.weight: button.primary ? Type.semibold : Type.medium
            color: button.primary ? Theme.color.textOnAccent : Theme.color.textSecondary
        }
    }
    FocusRing { onAccent: button.primary }
    HoverHandler {
        id: hover
        cursorShape: Qt.PointingHandCursor
        onHoveredChanged: if (button.tip !== button.text) {
            if (hovered) TipState.show(button.tip, button)
            else TipState.hide(button)
        }
    }
    TapHandler { gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: button.clicked() }
}
