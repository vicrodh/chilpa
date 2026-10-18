import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: integration
    required property var plugin
    signal command(string action, var input)
    spacing: Metrics.gap12
    Row {
        spacing: Metrics.gap12
        visible: integration.plugin.sidebar_position !== null && integration.plugin.sidebar_position !== undefined
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Position in Streaming", I18n.rev); font.pointSize: Type.body; color: Theme.color.textPrimary }
        Text { anchors.verticalCenter: parent.verticalCenter; text: String((integration.plugin.sidebar_position || 0) + 1) + " / " + String(integration.plugin.sidebar_count || 0); font.pointSize: Type.meta; color: Theme.color.textSecondary }
        IconButton { glyph: Icons.chevronUp; tip: I18n.tr("Move up", I18n.rev); enabled: integration.plugin.sidebar_position > 0; onClicked: integration.command("sidebar_position", {id:integration.plugin.id,position:integration.plugin.sidebar_position-1}) }
        IconButton { glyph: Icons.chevronDown; tip: I18n.tr("Move down", I18n.rev); enabled: integration.plugin.sidebar_position < integration.plugin.sidebar_count-1; onClicked: integration.command("sidebar_position", {id:integration.plugin.id,position:integration.plugin.sidebar_position+1}) }
    }
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
        visible: (integration.plugin.library_kinds || []).indexOf("albums") >= 0
        spacing: Metrics.gap12
        Toggle {
            checked: (integration.plugin.integration || {}).integrate_albums === true
            onToggled: (on) => integration.command("integration", {id: integration.plugin.id, field: "integrate_albums", enabled: on})
        }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Include source albums in Albums", I18n.rev); font.pointSize: Type.body; color: Theme.color.textPrimary }
    }
    Row {
        spacing: Metrics.gap12
        visible: (integration.plugin.library_kinds || []).indexOf("tracks") >= 0
        Toggle {
            checked: (integration.plugin.integration || {}).integrate_tracks === true
            onToggled: on => integration.command("integration", {id: integration.plugin.id, field: "integrate_tracks", enabled: on})
        }
        Text { text: I18n.tr("Show saved tracks in Tracks", I18n.rev); anchors.verticalCenter: parent.verticalCenter; font.pointSize: Type.body; color: Theme.color.textPrimary }
    }
    Row {
        spacing: Metrics.gap12
        visible: integration.plugin.native === true
        Toggle {
            checked: (integration.plugin.integration || {}).integrate_home === true
            onToggled: on => integration.command("integration", {id: integration.plugin.id, field: "integrate_home", enabled: on})
        }
        Text { text: I18n.tr("Show source recommendations by default in Home", I18n.rev); anchors.verticalCenter: parent.verticalCenter; font.pointSize: Type.body; color: Theme.color.textPrimary }
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
