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
        compare(backend.calls[backend.calls.length - 1], ["uninstall", {id: "guest"}])
        compare(manager.selected, "")
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
