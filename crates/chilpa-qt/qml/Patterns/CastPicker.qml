import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// I keep discovery scoped to the picker; the facade owns the session.
Modal {
    id: picker
    width: Metrics.confirmWidth
    property bool preview: false
    readonly property var castState: preview ? ({ devices: [{ id: "preview", name: "Living room", model: "Network renderer", kind: "DLNA / UPnP", state: I18n.tr("Available", I18n.rev) }], searching: false, active: false, error: "" }) : JSON.parse(Cast.snapshot || "{}")
    onOpened: if (!preview) Cast.picker(true)
    onClosed: if (!preview) Cast.picker(false)

    Timer {
        interval: Motion.refresh
        repeat: true
        running: picker.opened && !picker.preview
        onTriggered: Cast.refresh()
    }
    Column {
        width: parent.width
        spacing: Metrics.gap12
        Text {
            width: parent.width
            text: I18n.tr("Play on", I18n.rev)
            font.pointSize: Type.titleSmall
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: picker.castState.active ? picker.castState.name + " · " + picker.castState.state : I18n.tr("Choose a renderer on your local network.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Text {
            width: parent.width
            text: picker.castState.searching ? I18n.tr("Searching for devices…", I18n.rev) : I18n.tr("No devices found. Check that both devices use the same network.", I18n.rev)
            visible: picker.castState.searching || !(picker.castState.devices || []).length
            wrapMode: Text.WordWrap
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
        ListView {
            width: parent.width
            height: Math.min(contentHeight, Metrics.panesHeight)
            clip: true
            spacing: Metrics.gap8
            model: picker.castState.devices || []
            ScrollBar.vertical: ScrollBar {}
            delegate: Item {
                required property var modelData
                width: ListView.view.width
                height: Metrics.optionRow
                Column {
                    anchors.left: parent.left
                    anchors.right: connectButton.left
                    anchors.rightMargin: Metrics.gap12
                    anchors.verticalCenter: parent.verticalCenter
                    Text { width: parent.width; text: modelData.name; elide: Text.ElideRight; font.pointSize: Type.body; color: Theme.color.textPrimary }
                    Text { width: parent.width; text: modelData.kind + " · " + modelData.state; elide: Text.ElideRight; font.pointSize: Type.small; color: Theme.color.textSecondary }
                }
                PillButton {
                    id: connectButton
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: I18n.tr("Connect", I18n.rev)
                    enabled: !picker.preview
                    onClicked: Cast.connectDevice(modelData.id)
                }
            }
        }
        Text {
            width: parent.width
            text: I18n.tr("Original bytes · no transcoding by Chilpa · receiver processing unknown", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
        Text {
            width: parent.width
            visible: (picker.castState.error || "") !== ""
            text: picker.castState.error || ""
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textPrimary
        }
        Row {
            spacing: Metrics.gap8
            PillButton { text: I18n.tr("Refresh", I18n.rev); enabled: !picker.castState.searching && !picker.preview; onClicked: Cast.discover() }
            PillButton { text: I18n.tr("Disconnect", I18n.rev); visible: picker.castState.active || false; onClicked: Cast.disconnectDevice() }
            PillButton { text: I18n.tr("Done", I18n.rev); kind: "primary"; onClicked: picker.close() }
        }
    }
}
