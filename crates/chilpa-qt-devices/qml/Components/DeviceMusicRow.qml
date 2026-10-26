import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Item {
    id: row
    property var entry: ({})
    property int rowIndex: 0
    property bool checkable: false
    property bool busy: false
    signal toggled()
    signal opened()
    height: DeviceLayout.rowHeight

    ListRowSurface { anchors.fill: parent; rowIndex: row.rowIndex; selected: row.checkable && row.entry.checked === true; hovered: hover.hovered }
    HoverHandler { id: hover; onHoveredChanged: hovered && row.entry.kind === "track" && row.entry.path ? TipState.show(row.entry.path, row) : TipState.hide(row) }
    Component.onDestruction: TipState.hide(row)
    CircleCheck {
        id: check
        objectName: "deviceSelectionCheck"
        visible: row.checkable
        x: Metrics.gap4
        width: Metrics.button32
        height: parent.height
        Accessible.name: row.entry.title || ""
        checked: row.entry.checked === true
        enabled: !row.busy
        onClicked: row.toggled()
        HoverHandler { onHoveredChanged: hovered ? TipState.show(row.entry.title || "", check) : TipState.hide(check) }
        Component.onDestruction: TipState.hide(check)
    }
    Icon {
        visible: !row.checkable && !(row.entry.number > 0)
        x: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        glyph: row.entry.kind === "album" ? Icons.disc : row.entry.kind === "artist" ? Icons.user : row.entry.kind === "playlist" ? Icons.playlist : Icons.music
        size: Metrics.iconMd
        color: Theme.color.textSecondary
    }
    Text {
        visible: !row.checkable && row.entry.number > 0
        x: Metrics.gap4
        width: Metrics.button28
        anchors.verticalCenter: parent.verticalCenter
        horizontalAlignment: Text.AlignHCenter
        text: row.entry.number || ""
        color: Theme.color.textSecondary
        font.pointSize: Type.small
        font.features: { "tnum": 1 }
    }
    Controls.AbstractButton {
        id: content
        objectName: "deviceMusicOpen"
        x: row.checkable ? check.x + check.width : Metrics.gap8 + Metrics.iconMd + Metrics.gap8
        width: Math.max(0, parent.width - x - Metrics.gap8)
        height: parent.height
        enabled: row.entry.open === true && !row.busy
        focusPolicy: Qt.StrongFocus
        Accessible.name: row.entry.title || ""
        background: FocusRing { objectName: "deviceMusicFocusRing"; control: content }
        contentItem: Item {
            Column {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, parent.width - (arrow.visible ? Metrics.button28 : 0))
                spacing: Metrics.gap4
                Text { width: parent.width; text: row.entry.title || ""; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.body; font.weight: Type.medium }
                Text { width: parent.width; text: row.entry.kind === "playlist" ? row.entry.path || row.entry.detail || "" : row.entry.detail || row.entry.path || ""; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.small }
            }
            Icon { id: arrow; visible: row.entry.open === true; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; glyph: Icons.chevronRight; size: Metrics.iconSm; color: Theme.color.textSecondary }
        }
        HoverHandler { cursorShape: content.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
        onClicked: row.opened()
    }
}
