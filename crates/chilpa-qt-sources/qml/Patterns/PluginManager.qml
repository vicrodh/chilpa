import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as P
import "../Components"

Item {
    id: manager
    required property var page
    property string initialDownloadsId: ""
    property Component purchaseCard: null
    property var view: ({})
    property var settingsDocument: ({})
    property var catalogRows: []
    property var installedRows: []
    property string selected: ""
    readonly property var selectedPlugin: installedRows.find(p => p.id === selected) || null
    property string downloadsId: ""
    readonly property bool downloadsOpen: downloadsId !== ""
    property string proposalIdentity: ""
    property string tab: "uninspected"
    property string category: ""
    property bool filterPending: false
    property string section: "native"
    readonly property bool listing: !downloadsOpen && initialDownloadsId === ""
    property alias catalogView: compatibleTab.catalogView
    property alias detailsView: details
    function read() {
        const wasBusy = view.busy === true
        const text = page.pluginsJson()
        if (JSON.stringify(view) !== text) view = JSON.parse(text)
        if (wasBusy && view.busy === false) Sources.refresh()
        const rows = tab === "installed" ? view.installed_catalog || [] : view.catalog || []
        if (JSON.stringify(catalogRows) !== JSON.stringify(rows)) catalogRows = rows
        const hadSelection = selectedPlugin !== null
        if (JSON.stringify(installedRows) !== JSON.stringify(view.installed || [])) installedRows = view.installed || []
        if (hadSelection && selectedPlugin === null) { selected = ""; details.close() }
        if (JSON.stringify(settingsDocument) !== JSON.stringify(view.form || {})) settingsDocument = view.form || ({})
        const identity = (view.proposal || {}).id || ""
        if (identity !== proposalIdentity) {
            proposalIdentity = identity
            details.domains = ((view.proposal || {}).domains || []).join(", ")
        }
        if (view.approval && details.mode !== "permissions") { details.plugin = view.approval; details.mode = "permissions"; details.open() }
        if (!view.approval && details.mode === "permissions") { details.close(); details.mode = "settings" }
        if (filterPending && !view.busy && !settle.running) settle.start()
        if (initialDownloadsId !== "" && downloadsId !== initialDownloadsId && installedRows.some(p => p.id === initialDownloadsId && p.downloads === true && p.enabled === true))
            openDownloads({id: initialDownloadsId})
    }
    function command(action, input) { page.pluginCommand(action, JSON.stringify(input || {})); read() }
    function categoryLabel(value) {
        const labels = {musicservices: I18n.tr("Music services", I18n.rev), tools: I18n.tr("Tools", I18n.rev), information: I18n.tr("Information", I18n.rev), misc: I18n.tr("Other", I18n.rev), playlists: I18n.tr("Playlists", I18n.rev), hardware: I18n.tr("Hardware", I18n.rev), radio: I18n.tr("Radio", I18n.rev), scanning: I18n.tr("Library scanning", I18n.rev), skin: I18n.tr("Appearance", I18n.rev), uncategorized: I18n.tr("Uncategorized", I18n.rev)}
        return labels[value] || value
    }
    function filter() { filterPending = true; settle.restart() }
    function inspect(plugin) {
        details.mode = "inspection"
        details.plugin = plugin
        details.open()
        if ((view.proposal || {}).catalog_id !== plugin.id || (view.proposal || {}).repository_id !== plugin.repository_id)
            command("prepare", {repository: plugin.repository_id, id: plugin.id})
    }
    function settings(plugin) { details.mode = "settings"; details.tab = "settings"; details.plugin = plugin; selected = plugin.id; details.open(); if (plugin.enabled !== false && (plugin.settings || plugin.origin)) command("settings", {id: plugin.id}) }
    function openDownloads(plugin) { downloadsId = plugin.id; command("downloads_open", {id: plugin.id}) }
    function downloadAction(request) { command("downloads_action", {id: downloadsId, request: request}) }
    Component.onCompleted: read()
    Timer { interval: Motion.poll; repeat: true; running: manager.visible; onTriggered: manager.read() }
    Timer {
        objectName: "accountSetupPoll"
        interval: Motion.accountCheck
        repeat: true
        running: manager.visible && details.opened && manager.selected !== "" && details.tab === "settings"
            && manager.view.settings_id === manager.selected
            && manager.view.settings_loading_id !== manager.selected
            && ["pending", "running"].includes((manager.view.form || {}).status?.phase)
            && (manager.view.form.wizard || {}).waiting !== false
        onTriggered: manager.command("settings", {id: manager.selected})
    }
    Timer { interval: Motion.refresh; repeat: true; running: manager.visible && manager.downloadsOpen && !manager.view.busy; onTriggered: manager.command("downloads_tick", {id: manager.downloadsId}) }
    Timer {
        id: settle
        interval: Motion.searchSettle
        onTriggered: {
            if (manager.view.busy) { manager.filterPending = true; return }
            manager.filterPending = false
            manager.command("filter", {text: compatibleTab.searchText, category: manager.category, tab: manager.tab})
        }
    }
    PluginMaintenance { controller: manager }
    // Two sections, each one scroll: the native plugins, and the compatible
    // ones Chilpa can run from other platforms. A repository serves either.
    Item {
        id: header
        visible: manager.listing
        width: parent.width
        height: sections.height
        P.SegmentedControl {
            id: sections
            objectName: "pluginSections"
            options: [{text: I18n.tr("Native plugins", I18n.rev)}, {text: I18n.tr("Compatible plugins", I18n.rev)}]
            current: manager.section === "compatible" ? 1 : 0
            onActivated: (index) => manager.section = index === 1 ? "compatible" : "native"
        }
        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap8
            P.PillButton {
                objectName: "installLocalPlugin"
                visible: manager.section === "native"
                text: I18n.tr("Install from ZIP…", I18n.rev)
                enabled: !manager.view.busy
                onClicked: manager.command("choose_package", {})
            }
            P.PillButton { objectName: "managePluginRepositories"; text: I18n.tr("Repositories", I18n.rev); onClicked: repositories.open() }
        }
    }
    Text {
        id: status
        visible: manager.listing
        anchors.top: header.bottom
        anchors.topMargin: Metrics.gap8
        width: parent.width
        height: Metrics.fieldHeight
        text: manager.view.busy ? I18n.tr("Loading…", I18n.rev) : manager.view.error || ""
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        color: Theme.color.statusWarning
        font.pointSize: Type.body
    }
    PluginNativeTab {
        id: nativeTab
        controller: manager
        visible: manager.listing && manager.section === "native"
        anchors.top: status.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
    }
    PluginCompatibleTab {
        id: compatibleTab
        controller: manager
        visible: manager.listing && manager.section === "compatible"
        anchors.top: status.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
    }
    PluginManagerDownloads { anchors.fill: parent; controller: manager }
    PluginDownloadsUnavailable { anchors.centerIn: parent; visible: manager.initialDownloadsId !== "" && !manager.downloadsOpen; loading: manager.view.installed === undefined || manager.view.working === true; onBack: Navigation.goKey("settings", "plugins") }
    PluginDetails { id: details; manager: manager }
    PluginRepositories { id: repositories; manager: manager }
}
