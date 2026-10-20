import QtQuick
import QtTest
import "../../qml/Patterns"

TestCase {
    name: "PluginManager"
    when: windowShown
    width: 960
    height: 600
    visible: true
    QtObject {
        id: backend
        property var calls: []
        property var state: ({catalog: [{id: "entry", name: "Guest", light: "yellow", repository_id: "repo"}], installed: [], repositories: [], busy: false})
        function pluginsJson() { return JSON.stringify(state) }
        function pluginCommand(action, input) { calls = calls.concat([[action, JSON.parse(input)]]) }
    }
    PluginManager { id: manager; width: 600; height: 600; page: backend }
    function test_missing_genres_switch_defaults_off_and_uses_the_integration_command() {
        let plugin = {id: "fixture", name: "Source", native: true, enabled: true, integration_available: true, library_kinds: ["albums"]}
        backend.state = {installed: [plugin], busy: false}
        manager.read(); manager.settings(plugin)
        manager.detailsView.tab = "integration"
        let toggle = findChild(manager, "genresFromMusicBrainz")
        verify(toggle !== null)
        compare(toggle.checked, false)
        toggle.toggled(true)
        compare(backend.calls[backend.calls.length - 1], ["integration", {id: "fixture", field: "genres_from_musicbrainz", enabled: true}])
        plugin.integration = {genres_from_musicbrainz: true}
        backend.state = {installed: [plugin], busy: false}
        manager.read()
        tryCompare(toggle, "checked", true)
        toggle.toggled(false)
        compare(backend.calls[backend.calls.length - 1][1].enabled, false)
        manager.detailsView.close()
    }
    function test_missing_composers_switch_defaults_off_and_uses_the_integration_command() {
        let plugin = {id: "fixture", name: "Source", native: true, enabled: true, integration_available: true, library_kinds: ["albums"]}
        backend.state = {installed: [plugin], busy: false}
        manager.read(); manager.settings(plugin)
        manager.detailsView.tab = "integration"
        let toggle = findChild(manager, "composersFromMusicBrainz")
        verify(toggle !== null)
        compare(toggle.checked, false)
        toggle.toggled(true)
        compare(backend.calls[backend.calls.length - 1], ["integration", {id: "fixture", field: "composers_from_musicbrainz", enabled: true}])
        plugin.integration = {composers_from_musicbrainz: true}
        backend.state = {installed: [plugin], busy: false}
        manager.read()
        tryCompare(toggle, "checked", true)
        toggle.toggled(false)
        compare(backend.calls[backend.calls.length - 1][1].enabled, false)
        manager.detailsView.close()
    }
    function test_account_setup_polls_only_while_waiting() {
        const plugin = {id: "account", name: "Account", native: true, settings: true, enabled: true}
        backend.state = {installed: [plugin], settings_id: plugin.id, busy: false}
        manager.read(); manager.settings(plugin)
        const timer = findChild(manager, "accountSetupPoll")
        verify(timer !== null)
        for (const fixture of [
            ["ready", undefined, false], ["failed", undefined, false],
            ["expired", true, false], ["pending", false, false],
            ["pending", true, true], ["pending", undefined, true], ["running", true, true]
        ]) {
            backend.state = {installed: [plugin], settings_id: plugin.id, busy: false,
                form: {revision: 1, status: {phase: fixture[0]}, wizard: {waiting: fixture[1]},
                    values: {}, form: {fields: [], actions: []}}}
            manager.read()
            tryCompare(timer, "running", fixture[2])
        }
        manager.detailsView.close()
        tryCompare(timer, "running", false)
    }
    function test_maintenance_only_runs_for_enabled_compatibility_settings() {
        const timer = findChild(manager, "pluginMaintenance")
        verify(timer !== null)
        for (const plugin of [
            {id: "wasm", name: "WASM", native: false, origin: null, enabled: true},
            {id: "process", name: "Process", native: true, origin: null, enabled: true},
            {id: "disabled", name: "Disabled", native: false, origin: {}, enabled: false}
        ]) {
            backend.state = {installed: [plugin], busy: false, working: false}
            manager.read(); manager.settings(plugin)
            tryCompare(manager.detailsView, "opened", true)
            compare(timer.running, false, plugin.id)
            manager.detailsView.close()
            tryCompare(manager, "selected", "")
        }
        const plugin = {id: "bridge", name: "Bridge", origin: {}, enabled: true}
        backend.state = {installed: [plugin], busy: false, working: false}
        manager.read(); manager.settings(plugin)
        tryCompare(timer, "running", true)
        manager.detailsView.tab = "about"
        tryCompare(timer, "running", false)
        manager.detailsView.tab = "settings"
        tryCompare(timer, "running", true)
        manager.detailsView.close()
        tryCompare(timer, "running", false)
    }
    function test_removing_the_selected_plugin_stops_maintenance() {
        const plugin = {id: "removed", name: "Removed", origin: {}, enabled: true}
        backend.state = {installed: [plugin], busy: false, working: false}
        manager.read(); manager.settings(plugin)
        const timer = findChild(manager, "pluginMaintenance")
        tryCompare(timer, "running", true)
        backend.state = {installed: [], busy: false, working: false}
        manager.read()
        tryCompare(timer, "running", false)
        compare(manager.selected, "")
        verify(!manager.detailsView.opened)
    }
    function test_metadata_order_buttons_and_modal_tabs() {
        const plugin = {id: "ordered", name: "Source", version: "2.4", description: "Source description", enabled: true, light: "native", sidebar_position: 1, sidebar_count: 3}
        backend.state = {installed: [plugin], busy: false}
        manager.read()
        const list = findChild(manager, "installedNativePlugins")
        tryCompare(list, "count", 1)
        waitForRendering(list)
        const help = findChild(list, "pluginHelp")
        verify(help.text.includes("2.4") && help.text.includes("Source description"))
        findChild(list, "pluginMoveUp").clicked()
        compare(backend.calls[backend.calls.length - 1], ["sidebar_position", {id: "ordered", position: 0}])
        manager.settings(plugin)
        const modal = manager.detailsView
        const tabs = findChild(modal, "pluginTabs")
        const about = Array.from(tabs.children).find(item => item.objectName === "pluginTab_about")
        about.clicked()
        compare(about.kind, "primary")
        verify(findChild(modal, "pluginAbout").visible)
        Array.from(tabs.children).find(item => item.objectName === "pluginTab_integration").clicked()
        compare(about.kind, "secondary")
        verify(!findChild(modal, "pluginAbout").visible)
        const refresh = findChild(modal, "refreshPluginSettings")
        const uninstall = findChild(modal, "uninstallPlugin")
        compare(refresh.parent, uninstall.parent)
        refresh.clicked()
        compare(backend.calls[backend.calls.length - 1], ["settings", {id: "ordered"}])
        modal.close()
    }
    function test_installed_plugins_can_renew_network_permission_or_uninstall() {
        const plugin = {id: "guest", name: "Guest", origin: {}, state: "fine", enabled: true, limited_network: true}
        backend.state = {installed: [plugin], busy: false}
        manager.read(); manager.settings(plugin)
        const approve = findChild(manager.detailsView, "approvePluginNetwork")
        verify(approve.visible)
        approve.clicked()
        compare(backend.calls[backend.calls.length - 1], ["grant_network", {id: "guest", domain: "*"}])
        const remove = findChild(manager.detailsView, "uninstallPlugin")
        verify(remove.visible)
        remove.clicked()
        const confirmation = findChild(manager.detailsView, "confirmPluginRemoval")
        verify(confirmation.opened)
        confirmation.confirmed()
        confirmation.close()
        compare(backend.calls[backend.calls.length - 1], ["uninstall", {id: "guest"}])
        compare(manager.selected, "")
    }
    function test_permissions_require_explicit_acceptance() {
        backend.calls = []
        backend.state = {installed: [], busy: false, approval: {id: "native", name: "Native", description: "Fixture source", permissions: ["Network access"], revision: "fixture-revision"}}
        manager.read()
        compare(manager.detailsView.mode, "permissions")
        compare(backend.calls.length, 0)
        const accept = findChild(manager.detailsView, "acceptPluginPermissions")
        verify(accept.visible && accept.enabled)
        accept.clicked()
        compare(backend.calls[0], ["enable", {id: "native", permission_revision: "fixture-revision"}])
        backend.state = {installed: [], busy: false}
        manager.read()
        verify(!manager.detailsView.opened)
    }
    function test_native_settings_button_opens_its_form_and_submits_login() {
        const plugin = {id: "native", name: "Native", native: true, settings: true, enabled: true, light: "native"}
        backend.state = {busy: false, installed: [plugin]}
        backend.calls = []
        manager.read()
        const button = findChild(manager, "pluginSettingsButton")
        verify(button !== null && button.enabled)
        button.clicked()
        compare(manager.selected, "native")
        compare(backend.calls[0], ["settings", {id: "native"}])
        backend.state = {busy: false, installed: [plugin], settings_id: "native", form: {revision: 7, values: {}, form: {fields: [], actions: []}}}
        manager.read()
        const form = findChild(manager.detailsView, "pluginSettingsForm")
        verify(form.visible && form.enabled)
        form.submitted("sign_in", {app_id: "fixture"})
        compare(backend.calls[backend.calls.length - 1], ["submit", {id: "native", submission: {revision: 7, action: "sign_in", values: {app_id: "fixture"}}}])
        manager.detailsView.close()
    }
    function test_download_plugin_opens_a_catalogue_without_source_integration() {
        const plugin = {id: "download", name: "Purchases", native: true, settings: true, downloads: true,
                        integration_available: false, enabled: true, light: "native"}
        backend.state = {busy: false, installed: [plugin]}
        backend.calls = []
        manager.read()
        const open = findChild(manager, "openPluginDownloads")
        verify(open.visible && open.enabled)
        manager.initialDownloadsId = "download"
        manager.read()
        compare(backend.calls[backend.calls.length - 1], ["downloads_open", {id: "download"}])
        const albums = []
        for (let i = 0; i < 220; ++i)
            albums.push({id: String(i), title: "Album " + i, artist: "Artist"})
        backend.state = {busy: false, installed: [plugin], downloads_id: "download",
                         downloads: {signed_in: true, loaded: true, albums: albums, tracks: []}}
        manager.read()
        const grid = findChild(manager, "purchaseGrid")
        tryCompare(grid, "count", 220)
        verify(grid.visible)
        // The grid sizes its content when it lays the rows out, after the count.
        tryVerify(() => grid.contentHeight > grid.height, 3000)
        manager.settings(plugin)
        const tabs = findChild(manager.detailsView, "pluginTabs")
        verify(!Array.from(tabs.children).some(item => item.objectName === "pluginTab_integration"))
        manager.detailsView.close()
        manager.downloadsId = ""
        manager.initialDownloadsId = ""
    }
    function test_local_install_opens_the_picker_and_waits_while_busy() {
        backend.state = {busy: false}
        backend.calls = []
        manager.read()
        const button = findChild(manager, "installLocalPlugin")
        verify(button !== null && button.enabled)
        button.clicked()
        compare(backend.calls[0], ["choose_package", {}])
        backend.state = {busy: true}
        manager.read()
        verify(!button.enabled)
        backend.state = {busy: false}
        manager.read()
    }
    function test_every_native_plugin_shows_and_one_from_a_repository_installs() {
        const native = (id) => ({id: id, name: id, state: "fine", enabled: true})
        backend.state = {installed: [native("radio"), native("podcasts"), native("music")], busy: false,
            native_catalog: [{id: "app.test.one", name: "One", version: "0.2.0", description: "", repository_id: "repo", update: true}]}
        manager.read()
        const rows = findChild(manager, "nativeCatalogRow")
        verify(rows !== null, "a repository's native plugin is offered")
        let button = null
        for (let child of rows.children) if (child.text !== undefined && child.clicked !== undefined) button = child
        compare(button.text, "Update", "a newer version is offered as an update")
        button.clicked()
        compare(backend.calls[backend.calls.length - 1], ["install_native", {repository: "repo", id: "app.test.one"}])
        const installed = findChild(manager, "installedNativePlugins")
        tryCompare(installed, "count", 3)
        installed.positionViewAtEnd()
        waitForRendering(installed)
        verify(installed.contentHeight >= installed.height)
    }
    function test_catalog_command_keeps_repository_identity() {
        manager.command("prepare", {repository: "repo", id: "entry"})
        compare(backend.calls[backend.calls.length - 1], ["prepare", {repository: "repo", id: "entry"}])
    }
    function test_background_refresh_does_not_replace_an_unchanged_form() {
        backend.state = Object.assign({}, backend.state, {form: {revision: 1, values: {}, form: {fields: [], actions: []}}})
        manager.read()
        const original = manager.settingsDocument
        backend.state = Object.assign({}, backend.state, {busy: true})
        manager.read()
        compare(manager.settingsDocument, original)
        backend.state = Object.assign({}, backend.state, {busy: false, form: {revision: 2, values: {}, form: {fields: [], actions: []}}})
        manager.read()
        compare(manager.settingsDocument.revision, 2)
    }
    function test_large_catalog_only_creates_visible_rows() {
        const entries = []
        for (let i = 0; i < 220; ++i) entries.push({id: "entry" + i, name: "Guest " + i, light: "unknown", repository_id: "repo"})
        backend.state = {catalog: entries, installed: [], repositories: [], busy: false}
        manager.read()
        tryCompare(manager.catalogView, "count", 220)
        manager.catalogView.positionViewAtEnd()
        wait(100)
        let rows = 0
        for (let child of manager.catalogView.contentItem.children) if (child.objectName === "catalogPluginRow") ++rows
        verify(rows > 0 && rows < 40, "Only visible and pooled catalog rows should exist; got " + rows)
        manager.catalogView.positionViewAtBeginning()
    }

    function test_live_search_keeps_latest_text_while_busy() {
        const search = findChild(manager, "pluginSearch")
        verify(search !== null)
        backend.calls = []
        backend.state = Object.assign({}, backend.state, {busy: true})
        manager.read()
        search.text = "first"
        search.textEdited()
        search.text = "latest"
        search.textEdited()
        wait(350)
        compare(backend.calls.length, 0)
        backend.state = Object.assign({}, backend.state, {busy: false})
        manager.read()
        tryVerify(() => backend.calls.length > 0)
        compare(backend.calls[backend.calls.length - 1], ["filter", {text: "latest", category: "", tab: "uninspected"}])
        search.text = ""
    }

    function test_maintenance_keeps_settings_editable_without_loading() {
        backend.state = {busy: false, working: false, form: {revision: 1, values: {}, form: {fields: [], actions: []}}}
        manager.read()
        manager.settings({id: "guest", name: "Guest"})
        const status = findChild(manager.detailsView, "pluginStatus")
        const form = findChild(manager.detailsView, "pluginSettingsForm")
        verify(status !== null && form !== null)
        form.values = {account: "typed value"}
        for (let i = 0; i < 4; ++i) {
            backend.state = Object.assign({}, backend.state, {working: i % 2 === 0})
            manager.read()
            compare(status.text, "")
            verify(form.enabled)
            compare(form.values.account, "typed value")
        }
        manager.detailsView.close()
        compare(manager.selected, "")
    }

    function test_validation_result_precedes_permission_approval() {
        const plugin = {id: "entry", repository_id: "repo", name: "Guest"}
        backend.state = {busy: true, operation: "prepare"}
        manager.read()
        manager.inspect(plugin)
        const status = findChild(manager.detailsView, "validationStatus")
        const approve = findChild(manager.detailsView, "approvePlugin")
        verify(status !== null && approve !== null)
        compare(status.phase, "running")
        verify(!approve.visible)
        backend.state = {busy: false, validation: {id: "entry", repository_id: "repo", state: "passed"},
            proposal: {id: "installed-id", catalog_id: "entry", repository_id: "repo", domains: [], permissions: [], inspection: {limitations: []}}}
        manager.read()
        compare(status.phase, "passed")
        verify(approve.visible && approve.enabled)
        backend.state = {busy: false, validation: {id: "entry", repository_id: "repo", state: "failed", problem: "Initialization failed."}}
        manager.read()
        compare(status.phase, "failed")
        compare(status.problem, "Initialization failed.")
        verify(!approve.visible)
        manager.detailsView.close()
    }

    function test_toggle_retains_its_intrinsic_width_inside_a_full_width_form() {
        backend.state = {busy: false, form: {revision: 3, values: {enabled: true}, form: {
            fields: [{key: "enabled", label: "Enabled", control: {type: "toggle"}}], actions: []}}}
        manager.read()
        manager.settings({id: "guest", name: "Guest"})
        wait(30)
        const form = findChild(manager.detailsView, "pluginSettingsForm")
        tryVerify(() => findChild(form, "pluginToggle") !== null)
        const toggle = findChild(form, "pluginToggle")
        compare(toggle.width, toggle.implicitWidth)
        verify(toggle.width < 80)
        manager.detailsView.close()
    }

    function test_select_displays_the_option_label_and_submits_its_value() {
        backend.state = {busy: false, form: {revision: 2, values: {choice: "0"}, form: {
            fields: [{key: "choice", label: "Choice", control: {type: "select", choices: [{label: "Yes", value: "1"}, {label: "No", value: "0"}]}}], actions: []}}}
        manager.read()
        manager.settings({id: "guest", name: "Guest"})
        const form = findChild(manager.detailsView, "pluginSettingsForm")
        tryVerify(() => findChild(form, "pluginSelect") !== null)
        const select = findChild(form, "pluginSelect")
        compare(select.options[select.current], "No")
        select.picked("1")
        compare(select.options[select.current], "Yes")
        compare(form.values.choice, "1")
        manager.detailsView.close()
    }

}
