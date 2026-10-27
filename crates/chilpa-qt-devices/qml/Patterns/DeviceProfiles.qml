import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Modal {
    id: dialog
    objectName: "deviceProfiles"
    property var rows: []
    property bool busy: false
    property bool showOffline: false
    readonly property var displayedRows: visible ? rows.filter(row => showOffline || row.connected) : []
    signal forgetAsked(string id)
    width: Math.min(Metrics.settingsMaxWidth, parent ? parent.width - Metrics.modalInsets : Metrics.settingsMaxWidth)
    onOpened: showOffline = false

    Column {
        width: parent.width
        spacing: Metrics.gap12
        Text { text: I18n.tr("Manage profiles", I18n.rev); font.pointSize: Type.titleSmall; font.weight: Type.bold; color: Theme.color.textPrimary }
        CircleCheck {
            objectName: "deviceShowOfflineProfiles"
            text: I18n.tr("Show offline profiles", I18n.rev)
            checked: dialog.showOffline
            onClicked: dialog.showOffline = checked
        }
        Rectangle {
            id: table
            objectName: "deviceProfilesTable"
            width: parent.width
            height: header.height + Math.min(DeviceLayout.galleryHeight, Math.max(DeviceLayout.rowHeight, dialog.displayedRows.length * DeviceLayout.rowHeight)) + Metrics.gap4
            readonly property real statusWidth: Math.min(DeviceLayout.sizeWidth, width / 3) // derived: leave room for each column on narrow windows
            readonly property real actionWidth: Metrics.button32 + Metrics.gap16
            radius: Metrics.radiusMenu
            color: Theme.color.surfacePane
            border.width: Metrics.hairline
            border.color: Theme.color.strokeHairline
            Row {
                id: header
                objectName: "deviceProfilesHeader"
                x: Metrics.gap8
                width: list.width - Metrics.gap4
                height: DeviceLayout.tableHeader
                Text { width: Math.max(0, parent.width - table.statusWidth - table.actionWidth); height: parent.height; verticalAlignment: Text.AlignVCenter; text: I18n.tr("Profile", I18n.rev); elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.label }
                Text { width: table.statusWidth; height: parent.height; verticalAlignment: Text.AlignVCenter; text: I18n.tr("Status", I18n.rev); elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.label }
                Text { width: table.actionWidth; height: parent.height; verticalAlignment: Text.AlignVCenter; horizontalAlignment: Text.AlignHCenter; text: I18n.tr("Action", I18n.rev); elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.label }
            }
            Rectangle { x: Metrics.gap4; y: header.height; width: parent.width - Metrics.gap8; height: Metrics.hairline; color: Theme.color.strokeHairline }
            KeyedModel { id: profiles; keyField: "id"; items: dialog.displayedRows }
            ListView {
                id: list
                objectName: "deviceProfileList"
                x: Metrics.gap4
                y: header.height + Metrics.hairline
                width: parent.width - Metrics.gap8 - Metrics.gap16
                height: parent.height - y - Metrics.gap4
                clip: true
                model: profiles
                boundsBehavior: Flickable.StopAtBounds
                Controls.ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.gap4 }
                delegate: Item {
                    id: row
                    required property var entry
                    required property int index
                    width: list.width
                    height: DeviceLayout.rowHeight
                    ListRowSurface { anchors.fill: parent; rowIndex: row.index }
                    Text {
                        x: Metrics.gap4
                        width: Math.max(0, parent.width - x - table.statusWidth - table.actionWidth)
                        anchors.verticalCenter: parent.verticalCenter
                        text: row.entry.name || ""; textFormat: Text.PlainText; elide: Text.ElideRight
                        font.pointSize: Type.body; color: row.entry.connected ? Theme.color.textPrimary : Theme.color.textTertiary
                    }
                    Text {
                        objectName: "deviceProfileStatus"
                        x: parent.width - table.statusWidth - table.actionWidth
                        width: table.statusWidth
                        anchors.verticalCenter: parent.verticalCenter
                        text: row.entry.connected ? I18n.tr("Connected", I18n.rev) : I18n.tr("Offline", I18n.rev)
                        elide: Text.ElideRight; font.pointSize: Type.small; color: Theme.color.textSecondary
                    }
                    IconButton {
                        objectName: "deviceRemoveProfile"
                        x: parent.width - table.actionWidth + (table.actionWidth - width) / 2 // derived: center the action in its column
                        anchors.verticalCenter: parent.verticalCenter
                        glyph: Icons.trash
                        tip: I18n.tr("Remove saved profile", I18n.rev)
                        enabled: !dialog.busy
                        onClicked: { list.forceActiveFocus(); dialog.forgetAsked(row.entry.id) }
                    }
                }
            }
            Text { anchors.centerIn: list; visible: !dialog.displayedRows.length; text: dialog.showOffline ? I18n.tr("No saved profiles", I18n.rev) : I18n.tr("No connected profiles", I18n.rev); font.pointSize: Type.body; color: Theme.color.textSecondary }
        }
        PillButton { anchors.right: parent.right; text: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
    }
}
