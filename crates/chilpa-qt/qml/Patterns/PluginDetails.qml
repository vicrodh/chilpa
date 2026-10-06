import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "../Components"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives" as P

P.Modal {
    id: details
    required property var manager
    property var plugin: ({})
    property string mode: "inspection"
    property string tab: "settings"
    readonly property var validation: manager.view.validation || ({})
    readonly property bool matching: !!validation.id && validation.id === plugin.id && validation.repository_id === plugin.repository_id
    property alias domains: domains.text
    onClosed: { manager.selected = ""; if (mode === "permissions" && manager.view.approval) manager.command("cancel_permissions", {}) }
    width: Math.min(Metrics.settingsMaxWidth, parent ? parent.width - Metrics.modalInsets : Metrics.confirmWidth)
    height: Math.min(Metrics.panesHeight + Metrics.heroArt, parent ? parent.height - Metrics.modalInsets : Metrics.panesHeight)
    Column {
        id: heading
        width: parent.width
        spacing: Metrics.gap8
        Flow {
            objectName: "pluginTabs"
            width: parent.width
            spacing: Metrics.gap8
            visible: details.mode === "settings"
            Repeater {
                model: [{id: "about", title: I18n.tr("About", I18n.rev)}, {id: "settings", title: I18n.tr("Plugin Settings", I18n.rev)}, {id: "integration", title: I18n.tr("Plugin integration", I18n.rev)}]
                P.PillButton {
                    required property var modelData
                    objectName: "pluginTab_" + modelData.id
                    text: modelData.title
                    kind: details.tab === modelData.id ? "primary" : "secondary"
                    onClicked: details.tab = modelData.id
                }
            }
        }
        Text { text: details.plugin.name || ""; textFormat: Text.PlainText; font.pointSize: Type.titleSmall; color: Theme.color.textPrimary }
        PluginValidation {
            objectName: "validationStatus"
            width: parent.width
            visible: details.mode === "inspection"
            phase: details.manager.view.busy && details.manager.view.operation === "prepare" ? "running"
                : details.matching ? details.validation.state : "pending"
            problem: details.matching ? details.validation.problem || "" : ""
        }
        Text {
            objectName: "pluginStatus"
            width: parent.width
            text: details.manager.view.busy ? (details.manager.view.operation === "install" ? I18n.tr("Installing plugin…", I18n.rev) : I18n.tr("Loading…", I18n.rev)) : details.manager.view.error || ""
            visible: text !== "" && (details.mode === "settings" || details.manager.view.operation !== "prepare")
            wrapMode: Text.WordWrap
            color: Theme.color.statusWarning
            font.pointSize: Type.body
        }
    }
    Flickable {
        anchors.top: heading.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: footer.top
        anchors.bottomMargin: Metrics.gap12
        width: parent.width
        contentHeight: body.height
        clip: true
        ScrollBar.vertical: P.ViewportScrollBar { gutter: -Metrics.gap8;}
        Column {
            id: body
            width: parent.width - Metrics.gap12
            spacing: Metrics.gap12
            Text {
                objectName: "pluginAbout"
                visible: details.mode !== "settings" || details.tab === "about"
                width: parent.width
                text: [I18n.trArgs("Version: {}", [details.plugin.version || I18n.tr("Not specified", I18n.rev)], I18n.rev),
                    I18n.trArgs("Author: {}", [details.plugin.author || (details.plugin.origin || {}).catalog_entry?.author || I18n.tr("Not specified", I18n.rev)], I18n.rev),
                    details.plugin.description || I18n.tr("Description not provided yet.", I18n.rev), details.plugin.problem || ""].filter(Boolean).join("\n\n")
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                color: Theme.color.textSecondary
                font.pointSize: Type.body
            }
            Text {
                width: parent.width
                visible: details.mode === "permissions"
                text: (details.plugin.permissions || []).join("\n\n")
                textFormat: Text.PlainText; wrapMode: Text.WordWrap
                font.pointSize: Type.body; color: Theme.color.textPrimary
            }
            Column {
                width: parent.width
                visible: !!details.manager.view.proposal && details.manager.view.proposal.catalog_id === details.plugin.id && details.manager.view.proposal.repository_id === details.plugin.repository_id && !(details.manager.view.busy && details.manager.view.operation === "prepare")
                spacing: Metrics.gap8
                Text {
                    width: parent.width
                    text: ((details.manager.view.proposal || {}).permissions || []).join("\n")
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    color: Theme.color.textSecondary
                    font.pointSize: Type.body
                }
                P.TextField { id: domains; visible: false; width: parent.width; placeholder: I18n.tr("Allowed network domains, separated by commas", I18n.rev) }
                P.PillButton {
                    objectName: "approvePlugin"
                    text: I18n.tr("Approve permissions and install", I18n.rev)
                    enabled: !details.manager.view.busy
                    onClicked: details.manager.command("install", {domains: domains.text.split(",").map(v => v.trim()).filter(Boolean)})
                }
            }
            Text {
                width: parent.width
                visible: details.mode === "inspection" && details.matching && details.validation.state === "passed" && text !== ""
                text: ((details.manager.view.proposal || {}).inspection?.limitations || []).join("\n")
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                color: Theme.color.textSecondary
                font.pointSize: Type.small
            }
            P.PillButton {
                visible: details.mode === "settings" && !!details.manager.view.error
                text: I18n.tr("Restart plugin", I18n.rev)
                enabled: !details.manager.view.busy
                onClicked: details.manager.command("restart", {id: details.plugin.id})
            }
            PluginIntegration {
                width: parent.width
                visible: details.mode === "settings" && details.tab === "integration"
                plugin: (details.manager.view.installed || []).find(p => p.id === details.plugin.id) || details.plugin
                onCommand: (action, input) => details.manager.command(action, input)
            }
            Column {
                width: parent.width
                visible: details.tab === "settings" && details.manager.selected === details.plugin.id
                spacing: Metrics.gap8
                PluginSettingsForm {
                    objectName: "pluginSettingsForm"
                    width: parent.width
                    visible: !!details.manager.view.form && details.manager.view.settings_id === details.plugin.id
                    document: details.manager.settingsDocument
                    enabled: !details.manager.view.busy
                    onSubmitted: (action, values) => details.manager.command("submit", {id: details.plugin.id, submission: {revision: details.manager.view.form.revision, action: action, values: values}})
                    onLinked: (url) => {
                        if (/^https?:\/\//i.test(url)) Qt.openUrlExternally(url)
                        else details.manager.command("settings", {id: details.plugin.id, page: url.replace(/^\//, "")})
                    }
                }
            }
            Column {
                width: parent.width
                spacing: Metrics.gap8
                visible: details.mode === "settings" && ((details.manager.view.installed || []).find(p => p.id === details.plugin.id) || {}).limited_network === true
                Text {
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: I18n.tr("Network access, including authentication, artwork and audio servers discovered by the plugin", I18n.rev)
                    color: Theme.color.textSecondary
                    font.pointSize: Type.body
                }
                P.PillButton {
                    objectName: "approvePluginNetwork"
                    text: I18n.tr("Approve network permission", I18n.rev)
                    enabled: !details.manager.view.busy
                    onClicked: details.manager.command("grant_network", {id: details.plugin.id, domain: "*"})
                }
            }
        }
    }
    Item {
        id: footer
        width: parent.width
        anchors.bottom: parent.bottom
        height: Math.max(actions.implicitHeight, close.implicitHeight)
        Flow {
            id: actions
            width: parent.width - close.width - Metrics.gap12
            spacing: Metrics.gap8
            P.PillButton {
                objectName: "uninstallPlugin"
                visible: details.mode === "settings"
                enabled: !details.manager.view.busy
                text: I18n.tr("Uninstall plugin and remove account data", I18n.rev)
                onClicked: removal.open()
            }
            P.PillButton {
                objectName: "refreshPluginSettings"
                visible: details.mode === "settings"
                text: I18n.tr("Refresh settings", I18n.rev)
                glyph: Lucide.refreshCcwDot
                iconFamily: Lucide.font
                enabled: !details.manager.view.busy
                onClicked: details.manager.command("settings", {id: details.plugin.id})
            }
            P.PillButton {
                objectName: "acceptPluginPermissions"
                visible: details.mode === "permissions"
                text: I18n.tr("Accept permissions and enable", I18n.rev)
                enabled: !details.manager.view.busy && !!details.plugin.revision
                onClicked: details.manager.command("enable", {id: details.plugin.id, permission_revision: details.plugin.revision})
            }
        }
        P.PillButton { id: close; anchors.right: parent.right; text: I18n.tr("Close", I18n.rev); onClicked: details.close() }
    }
    ConfirmDialog {
        id: removal
        objectName: "confirmPluginRemoval"
        title: I18n.tr("Uninstall plugin", I18n.rev)
        message: I18n.tr("Remove this plugin, its account, settings and cached library from Chilpa? This does not delete your account or playlists at the source.", I18n.rev)
        confirmText: I18n.tr("Uninstall", I18n.rev)
        destructive: true
        onConfirmed: { details.manager.command("uninstall", {id: details.plugin.id}); details.close() }
    }
}
