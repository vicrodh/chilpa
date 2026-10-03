import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "../Theme"
import "../Primitives" as P
import "../Components"

Item {
    id: manager
    required property var page
    property var view: ({})
    property var settingsDocument: ({})
    property var catalogRows: []
    property var installedRows: []
    property string selected: ""
    property string proposalIdentity: ""
    property string tab: "uninspected"
    property string category: ""
    property bool filterPending: false
    property alias catalogView: catalog
    property alias detailsView: details
    function read() {
        const wasBusy = view.busy === true
        const text = page.pluginsJson()
        if (JSON.stringify(view) !== text) view = JSON.parse(text)
        if (wasBusy && view.busy === false) Sources.refresh()
        const rows = tab === "installed" ? view.installed_catalog || [] : view.catalog || []
        if (JSON.stringify(catalogRows) !== JSON.stringify(rows)) catalogRows = rows
        if (JSON.stringify(installedRows) !== JSON.stringify(view.installed || [])) installedRows = view.installed || []
        if (JSON.stringify(settingsDocument) !== JSON.stringify(view.form || {})) settingsDocument = view.form || ({})
        const identity = (view.proposal || {}).id || ""
        if (identity !== proposalIdentity) {
            proposalIdentity = identity
            details.domains = ((view.proposal || {}).domains || []).join(", ")
        }
        if (view.approval && details.mode !== "permissions") { details.plugin = view.approval; details.mode = "permissions"; details.open() }
        if (!view.approval && details.mode === "permissions") { details.close(); details.mode = "settings" }
        if (filterPending && !view.busy && !settle.running) settle.start()
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
    Component.onCompleted: read()
    Timer { interval: Motion.poll; repeat: true; running: manager.visible; onTriggered: manager.read() }
    Timer {
        id: settle
        interval: Motion.searchSettle
        onTriggered: {
            if (manager.view.busy) { manager.filterPending = true; return }
            manager.filterPending = false
            manager.command("filter", {text: search.text, category: manager.category, tab: manager.tab})
        }
    }
    Timer {
        interval: Motion.refresh
        repeat: true
        running: manager.visible && manager.selected !== "" && !(manager.view.installed || []).some(p => p.id === manager.selected && p.native === true) && !manager.view.maintenance_failed && !manager.view.working && !manager.view.busy && !manager.filterPending
        onTriggered: manager.command("tick", {id: manager.selected})
    }
    Column {
        id: toolbar
        width: parent.width
        spacing: Metrics.gap12
        // A repository serves either section: native plugins, or a catalog
        // of compatible ones; Chilpa tells them apart.
        Row {
            width: parent.width
            spacing: Metrics.gap8
            P.TextField {
                id: repository
                width: Math.max(0, parent.width - add.width - manage.width - Metrics.gap8 * 2) // derived: two buttons and two gaps
                placeholder: I18n.tr("Plugin repository URL", I18n.rev)
            }
            P.PillButton { id: add; text: I18n.tr("Add", I18n.rev); enabled: !manager.view.busy; onClicked: manager.command("add", {url: repository.text}) }
            P.PillButton { id: manage; text: I18n.tr("Manage", I18n.rev); onClicked: repositories.open() }
        }
        P.PillButton {
            objectName: "installLocalPlugin"
            text: I18n.tr("Install from ZIP…", I18n.rev)
            enabled: !manager.view.busy
            onClicked: manager.command("choose_package", {})
        }
        Text { text: I18n.tr("Chilpa Native Plugins", I18n.rev); font.pointSize: Type.titleSmall; color: Theme.color.textPrimary }
        Repeater {
            model: manager.installedRows.filter(p => !p.origin)
            delegate: PluginRow {
                required property var modelData
                width: toolbar.width - Metrics.gap12
                plugin: modelData
                installed: true
                enabled: !manager.view.busy
                onSettings: manager.settings(modelData)
                onSwitched: (on) => manager.command(on ? "review_permissions" : "disable", {id: modelData.id})
            }
        }
        // What the added repositories offer that is not installed, or is
        // newer than what is. Installed, it waits switched off.
        Repeater {
            model: manager.view.native_catalog || []
            delegate: Item {
                required property var modelData
                objectName: "nativeCatalogRow"
                width: toolbar.width - Metrics.gap12
                height: Metrics.settingsRow
                Column {
                    anchors.left: parent.left
                    anchors.right: get.left
                    anchors.rightMargin: Metrics.gap12
                    anchors.verticalCenter: parent.verticalCenter
                    Text { width: parent.width; text: modelData.name + "  " + modelData.version; elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.body }
                    Text { width: parent.width; text: modelData.problem || modelData.description; elide: Text.ElideRight; color: modelData.problem ? Theme.color.statusWarning : Theme.color.textSecondary; font.pointSize: Type.meta }
                }
                P.PillButton {
                    id: get
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData.update ? I18n.tr("Update", I18n.rev) : I18n.tr("Install", I18n.rev)
                    enabled: !manager.view.busy && !modelData.problem
                    onClicked: manager.command("install_native", {repository: modelData.repository_id, id: modelData.id})
                }
            }
        }
        Text { text: I18n.tr("Compatible plugins", I18n.rev); font.pointSize: Type.titleSmall; color: Theme.color.textPrimary }
        Text {
            width: parent.width
            text: I18n.tr("Chilpa can run some plugins made for other platforms. Add a plugin catalog you use. Compatibility is not guaranteed; Chilpa is not responsible for these plugins' behavior.", I18n.rev)
            wrapMode: Text.WordWrap
            color: Theme.color.textSecondary
            font.pointSize: Type.body
        }
        Flow {
            width: parent.width
            spacing: Metrics.gap8
            Repeater {
                model: [{id: "installed", label: I18n.tr("Installed", I18n.rev)}, {id: "valid", label: I18n.tr("Valid", I18n.rev)}, {id: "uninspected", label: I18n.tr("Not inspected", I18n.rev)}, {id: "incompatible", label: I18n.tr("Not compatible", I18n.rev)}]
                P.PillButton {
                    required property var modelData
                    text: modelData.label + " (" + ((manager.view.counts || {})[modelData.id] || 0) + ")"
                    kind: manager.tab === modelData.id ? "primary" : "secondary"
                    onClicked: { manager.tab = modelData.id; manager.filter() }
                }
            }
        }
        Row {
            width: parent.width
            spacing: Metrics.gap8
            P.TextField {
                id: search
                objectName: "pluginSearch"
                width: Math.max(0, parent.width - categories.width - Metrics.gap8)
                placeholder: I18n.tr("Search plugins", I18n.rev)
                onTextEdited: manager.filter()
                onAccepted: manager.filter()
            }
            P.SelectField {
                id: categories
                width: Math.min(Metrics.selectWidth, parent.width / 2) // derived: reserve at least half for search
                options: [I18n.tr("All categories", I18n.rev)].concat((manager.view.categories || []).map(c => manager.categoryLabel(c)))
                values: [""].concat(manager.view.categories || [])
                value: manager.category
                onPicked: (value) => { manager.category = value; manager.filter() }
            }
        }
        Text {
            width: parent.width
            text: manager.view.busy ? I18n.tr("Loading…", I18n.rev) : manager.view.error || ""
            height: Metrics.fieldHeightLarge
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
            color: Theme.color.statusWarning
            font.pointSize: Type.body
        }
    }
    ListView {
        id: catalog
        anchors.top: toolbar.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        clip: true
        reuseItems: true
        model: manager.catalogRows
        ScrollBar.vertical: ScrollBar {}
        delegate: PluginRow {
            required property var modelData
            objectName: "catalogPluginRow"
            width: catalog.width - Metrics.gap12
            plugin: modelData
            installed: manager.tab === "installed"
            enabled: !manager.view.busy && !manager.filterPending
            onInspected: manager.inspect(modelData)
            onSettings: manager.settings(modelData)
            onSwitched: (on) => manager.command(on ? "review_permissions" : "disable", {id: modelData.id})
        }
        Text {
            anchors.centerIn: parent
            visible: catalog.count === 0 && !manager.view.busy
            text: I18n.tr("No matching plugins", I18n.rev)
            color: Theme.color.textSecondary
            font.pointSize: Type.body
        }
    }
    PluginDetails { id: details; manager: manager }
    PluginRepositories { id: repositories; manager: manager }
}
