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
    width: Math.min(Metrics.confirmWidth, parent ? parent.width - Metrics.modalInsets : Metrics.confirmWidth)
    onOpened: showOffline = false

    Column {
        width: parent.width
        spacing: Metrics.gap12
        Text { text: I18n.tr("Manage profiles", I18n.rev); font.pointSize: Type.titleSmall; font.weight: Type.bold; color: Theme.color.textPrimary }
        PillButton { objectName: "deviceShowOfflineProfiles"; text: dialog.showOffline ? I18n.tr("Hide offline profiles", I18n.rev) : I18n.tr("Show offline profiles", I18n.rev); onClicked: dialog.showOffline = !dialog.showOffline }
        Item {
            width: parent.width
            height: Math.min(DeviceLayout.galleryHeight, Math.max(DeviceLayout.rowHeight, dialog.displayedRows.length * DeviceLayout.rowHeight))
            ListView {
                id: list
                objectName: "deviceProfileList"
                anchors.fill: parent
                clip: true
                model: dialog.displayedRows
                Controls.ScrollBar.vertical: ViewportScrollBar {}
                delegate: Item {
                    id: row
                    required property var modelData
                    required property int index
                    width: list.width
                    height: DeviceLayout.rowHeight
                    ListRowSurface { anchors.fill: parent; rowIndex: row.index }
                    Column {
                        x: Metrics.gap8
                        width: Math.max(0, parent.width - remove.width - Metrics.gap20)
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: Metrics.gap4
                        Text { width: parent.width; text: row.modelData.name || ""; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.body; color: row.modelData.connected ? Theme.color.textPrimary : Theme.color.textTertiary }
                        Text { text: row.modelData.connected ? I18n.tr("Connected", I18n.rev) : I18n.tr("Offline", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
                    }
                    IconButton { id: remove; objectName: "deviceRemoveProfile"; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; glyph: Icons.trash; tip: I18n.tr("Remove saved profile", I18n.rev); enabled: !dialog.busy; onClicked: dialog.forgetAsked(row.modelData.id) }
                }
            }
            Text { anchors.centerIn: parent; visible: !dialog.displayedRows.length; text: I18n.tr("No connected profiles", I18n.rev); font.pointSize: Type.body; color: Theme.color.textSecondary }
        }
        PillButton { anchors.right: parent.right; text: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
    }
}
