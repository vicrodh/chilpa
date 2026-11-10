import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as P
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"

P.Modal {
    id: details
    required property var manager
    property var plugin: ({})
    property string mode: "inspection"
    property string tab: "settings"
    readonly property bool settingsLoading: manager.view.settings_loading_id === plugin.id
    readonly property var validation: manager.view.validation || ({})
    readonly property bool matching: !!validation.id && validation.id === plugin.id && validation.repository_id === plugin.repository_id
    property alias domains: domains.text
    onClosed: { manager.selected = ""; if (mode === "permissions" && manager.view.approval) manager.command("cancel_permissions", {}) }
    preferredWidth: Math.min(Metrics.settingsMaxWidth, parent ? parent.width - Metrics.modalInsets : Metrics.confirmWidth)
    Column {
        width: parent.width
        spacing: Metrics.gap12
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
                    model: [{id: "about", title: I18n.tr("About", I18n.rev)}, {id: "settings", title: details.plugin.downloads === true ? I18n.tr("Account", I18n.rev) : I18n.tr("Plugin Settings", I18n.rev)}]
                        .concat(details.plugin.integration_available === false ? [] : [{id: "integration", title: I18n.tr("Plugin integration", I18n.rev)}])
                    P.PillButton {
                        required property var modelData
                        objectName: "pluginTab_" + modelData.id
                        text: modelData.title
                        kind: details.tab === modelData.id ? "primary" : "secondary"
                        onClicked: details.tab = modelData.id
                    }
                }
            }
            Text { width: parent.width; text: details.plugin.name || ""; textFormat: Text.PlainText; wrapMode: Text.WrapAtWordBoundaryOrAnywhere; font.pointSize: Type.titleSmall; color: Theme.color.textPrimary }
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
                text: details.manager.view.busy || details.settingsLoading
                    ? (details.manager.view.operation === "install" ? I18n.tr("Installing plugin…", I18n.rev) : I18n.tr("Loading…", I18n.rev))
                    : details.manager.view.error || ""
                visible: text !== "" && (details.mode === "settings" || details.manager.view.operation !== "prepare")
                wrapMode: Text.WordWrap
                color: Theme.color.statusWarning
                font.pointSize: Type.body
            }
        }
        Column {
            id: body
            width: parent.width
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
                visible: details.mode === "settings" && details.tab === "integration" && details.plugin.integration_available !== false
                plugin: (details.manager.view.installed || []).find(p => p.id === details.plugin.id) || details.plugin
                onCommand: (action, input) => details.manager.command(action, input)
            }
            Column {
                width: parent.width
                visible: details.tab === "settings" && details.manager.selected === details.plugin.id
                spacing: Metrics.gap8
                Text {
                    width: parent.width
                    visible: ((details.manager.view.installed || []).find(p => p.id === details.plugin.id) || {}).folder_write === true
                    text: ((details.manager.view.installed || []).find(p => p.id === details.plugin.id) || {}).export_folder || I18n.tr("No download folder selected", I18n.rev)
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                    color: Theme.color.textSecondary
                    font.pointSize: Type.body
                }
                P.PillButton {
                    objectName: "choosePluginExportFolder"
                    visible: ((details.manager.view.installed || []).find(p => p.id === details.plugin.id) || {}).folder_write === true
                    text: I18n.tr("Choose download folder…", I18n.rev)
                    enabled: !details.manager.view.busy
                    onClicked: details.manager.command("choose_export_folder", {id: details.plugin.id})
                }
                PluginSettingsForm {
                    pluginName: details.plugin.name || details.plugin.id || ""
                    pluginId: details.plugin.id || ""
                    objectName: "pluginSettingsForm"
                    width: parent.width
                    visible: !!details.manager.view.form && details.manager.view.settings_id === details.plugin.id
                    document: details.manager.settingsDocument
                    enabled: !details.manager.view.busy && !details.settingsLoading
                    onSubmitted: (action, values, scope) => details.manager.command("submit", {id: scope.plugin, submission: {revision: scope.revision, action: action, values: values}})
                    captures: details.manager.view.web_sign_in === true
                    onLinked: (url) => /^https?:\/\//i.test(url) ? Qt.openUrlExternally(url) : details.manager.command("settings", {id: details.plugin.id, page: url.replace(/^\//, "")})
                    onCaptured: (link) => details.manager.command("web_sign_in", {id: details.plugin.id, url: link.url, capture: link.capture,
                        revision: details.manager.view.form.revision})
                }
                Text {
                    visible: details.plugin.downloads === true
                    text: I18n.tr("Access to downloader", I18n.rev)
                    color: Theme.color.textPrimary; font.pointSize: Type.body
                }
                P.SelectField {
                    objectName: "downloadAccessSelect"
                    visible: details.plugin.downloads === true
                    width: Math.min(parent.width, Metrics.selectWidth * 2) // derived: long location labels
                    options: [I18n.tr("From plugin", I18n.rev), I18n.tr("From settings menu", I18n.rev), I18n.tr("From main sidebar", I18n.rev)]
                    values: ["plugin", "settings", "sidebar"]
                    value: (((details.manager.installedRows || []).find(p => p.id === details.plugin.id) || {}).download_access || "plugin")
                    onPicked: value => details.manager.command("download_access", {id: details.plugin.id, value: value})
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
        PluginDialogActions {
            id: footer
            width: parent.width
            plugin: details.plugin
            view: details.manager.view
            mode: details.mode
            onUninstallRequested: removal.open()
            onRefreshRequested: details.manager.command("settings", {id: details.plugin.id})
            onCloseRequested: details.close()
            onPermissionAccepted: details.manager.command(details.plugin.local_update ? "install_local" : details.plugin.update ? "install_native" : "enable", details.plugin.update
                ? {id: details.plugin.id, repository: details.plugin.repository, path: details.plugin.path, permission_revision: details.plugin.revision}
                : {id: details.plugin.id, permission_revision: details.plugin.revision})
        }
    }
    ConfirmDialog {
        id: removal; objectName: "confirmPluginRemoval"
        title: I18n.tr("Uninstall plugin", I18n.rev)
        message: I18n.tr("Remove this plugin, its account, settings and cached library from Chilpa? This does not delete your account or playlists at the source.", I18n.rev)
        confirmText: I18n.tr("Uninstall", I18n.rev)
        option: I18n.tr("Also delete the lyrics saved for this plugin's tracks", I18n.rev)
        destructive: true
        onConfirmed: { details.manager.command("uninstall", {id: details.plugin.id, lyrics: removal.optionChecked}); details.close() }
    }
}
