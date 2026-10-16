import QtQuick
import QtQuick.Controls.Basic as Controls
import "../Theme"

// Selection uses the same circle glyph as a selected media row.
Controls.CheckBox {
    id: control
    implicitHeight: Math.max(Metrics.button32, contentItem.implicitHeight + Metrics.gap8)
    padding: Metrics.gap4
    spacing: Metrics.gap8
    indicator: Icon {
        x: control.padding; anchors.verticalCenter: parent.verticalCenter
        glyph: control.checked ? Icons.circleCheckFilled : Icons.circle
        filled: control.checked; size: Metrics.iconMd
        color: control.checked || control.activeFocus ? Theme.color.accent : Theme.color.textSecondary
    }
    contentItem: Text {
        text: control.text; leftPadding: control.indicator.width + control.spacing
        verticalAlignment: Text.AlignVCenter; wrapMode: Text.Wrap
        color: Theme.color.textPrimary; font.pointSize: Type.body
    }
    opacity: enabled ? 1 : Metrics.disabledOpacity
    HoverHandler { cursorShape: Qt.PointingHandCursor }
}
