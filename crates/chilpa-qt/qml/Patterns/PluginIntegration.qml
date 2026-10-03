import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

Column {
    id: integration
    required property var plugin
    signal command(string action, var input)
    spacing: Metrics.gap12
    Text { text: I18n.tr("Icon", I18n.rev); font.pointSize: Type.body; color: Theme.color.textPrimary }
    Row {
        spacing: Metrics.gap8
        PillButton { text: I18n.tr("Choose image", I18n.rev); onClicked: integration.command("choose_icon", {id: integration.plugin.id}) }
        PillButton { text: I18n.tr("Restore default", I18n.rev); onClicked: integration.command("icon", {id: integration.plugin.id, path: ""}) }
    }
    Row {
        spacing: Metrics.gap12
        Toggle { checked: integration.plugin.tint === true; onToggled: (on) => integration.command("tint", {id: integration.plugin.id, enabled: on}) }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Apply system tint to the icon", I18n.rev); font.pointSize: Type.body; color: Theme.color.textPrimary }
    }
    Row {
        spacing: Metrics.gap12
        Toggle {
            checked: (integration.plugin.integration || {}).separate_playlists === true
            onToggled: (on) => integration.command("integration", {id: integration.plugin.id, field: "separate_playlists", enabled: on})
        }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Separate source playlists", I18n.rev); font.pointSize: Type.body; color: Theme.color.textPrimary }
    }
    Row {
        visible: integration.plugin.native === true
        spacing: Metrics.gap12
        Toggle {
            checked: (integration.plugin.integration || {}).integrate_albums === true
            onToggled: (on) => integration.command("integration", {id: integration.plugin.id, field: "integrate_albums", enabled: on})
        }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Include source albums in Albums", I18n.rev); font.pointSize: Type.body; color: Theme.color.textPrimary }
    }
    Text { visible: integration.plugin.native !== true; text: I18n.tr("Library integration", I18n.rev); font.pointSize: Type.body; color: Theme.color.textPrimary }
    Text {
        width: parent.width
        visible: integration.plugin.native !== true
        text: I18n.tr("Open a collection in the source and choose Add collection to library. Imported music appears in Albums and Tracks with its source.", I18n.rev)
        wrapMode: Text.WordWrap
        color: Theme.color.textSecondary
        font.pointSize: Type.small
    }
    Row {
        visible: integration.plugin.native !== true
        spacing: Metrics.gap12
        Toggle {
            enabled: integration.plugin.library_count > 0
            checked: integration.plugin.library_visible === true
            onToggled: (on) => integration.command("library_visibility", {id: integration.plugin.id, enabled: on})
        }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Show imported albums and tracks", I18n.rev); font.pointSize: Type.body; color: Theme.color.textPrimary }
    }

}
