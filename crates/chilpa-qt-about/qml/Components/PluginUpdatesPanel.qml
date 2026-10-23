import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.about
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as P

Column {
    id: panel
    property var review: function() {}
    readonly property var state: JSON.parse(PluginUpdates.snapshot || "{}")
    spacing: Metrics.gap12
    Text { text: I18n.tr("Plugin updates", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.titleSmall }
    Row {
        spacing: Metrics.gap8
        P.Toggle { objectName: "automaticPluginUpdates"; checked: panel.state.automatic === true; onToggled: on => PluginUpdates.command("updates_automatic", JSON.stringify({enabled:on})) }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Automatically update plugins", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.body }
    }
    Text {
        width: parent.width; wrapMode: Text.WordWrap; color: Theme.color.textSecondary; font.pointSize: Type.body
        text: I18n.tr("Allow updates with the same permissions while playback is stopped. Updates that need more permissions wait for review. Your accounts and settings are kept.", I18n.rev)
    }
    Row {
        spacing: Metrics.gap8
        P.PillButton { objectName: "checkPluginUpdates"; text: I18n.tr("Check for updates", I18n.rev); onClicked: PluginUpdates.command("updates_check", "{}") }
        P.PillButton { objectName: "applyPluginUpdates"; text: I18n.tr("Update all", I18n.rev); enabled: (panel.state.items || []).some(item => ["available", "deferred", "failed"].includes(item.phase)); onClicked: PluginUpdates.command("updates_apply", "{}") }
    }
    Text { visible: !!panel.state.error; width: parent.width; wrapMode: Text.WordWrap; text: panel.state.error || ""; color: Theme.color.textSecondary; font.pointSize: Type.body }
    Text { visible: !(panel.state.items || []).length; text: I18n.tr("No plugin updates available.", I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.body }
    ListView {
        objectName: "pluginUpdateInbox"
        width: parent.width
        height: Math.min(contentHeight, Metrics.menuMaxWidth)
        clip: true
        model: panel.state.items || []
        spacing: Metrics.gap8
        ScrollBar.vertical: P.ViewportScrollBar {}
        delegate: Column {
            required property var modelData
            width: ListView.view.width
            spacing: Metrics.gap4
            Text { width: parent.width; elide: Text.ElideRight; text: modelData.name + " · " + modelData.version; color: Theme.color.textPrimary; font.pointSize: Type.body }
            Text {
                objectName: "pluginNoticeStatus"
                width: parent.width; wrapMode: Text.WordWrap; color: Theme.color.textSecondary; font.pointSize: Type.small
                text: modelData.phase === "new" ? modelData.third_party === true ? I18n.tr("New third-party plugin", I18n.rev) : I18n.tr("New plugin", I18n.rev)
                    : modelData.phase === "updated" ? I18n.tr("Updated", I18n.rev)
                    : modelData.phase === "deferred" ? I18n.tr("Waiting for playback to stop", I18n.rev)
                    : modelData.phase === "review" ? I18n.tr("Permission review required", I18n.rev)
                    : modelData.phase === "failed" ? I18n.tr("Update failed; the previous version is kept", I18n.rev)
                    : I18n.tr("Update available", I18n.rev)
            }
            Text { visible: !!modelData.message; width: parent.width; wrapMode: Text.WordWrap; text: modelData.message; color: Theme.color.textSecondary; font.pointSize: Type.small }
            P.PillButton { visible: modelData.phase === "review"; text: I18n.tr("Review", I18n.rev); onClicked: panel.review() }
            P.PillButton { objectName: "openNewPlugin"; visible: modelData.phase === "new"; text: I18n.tr("View", I18n.rev); onClicked: panel.review() }
        }
    }
    Component.onCompleted: PluginUpdates.refresh()
}
