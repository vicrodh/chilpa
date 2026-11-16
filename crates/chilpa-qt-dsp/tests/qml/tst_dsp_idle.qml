import QtQuick
import QtQuick.Window
import QtTest
import app.chilpa.dsp
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"

TestCase {
    id: suite
    name: "DspIdleStatus"
    when: windowShown
    visible: true
    width: 1440
    height: 1000
    DspControl { id: control }
    Component { id: panelFactory; DspPanel {} }
    function init() {
        suite.Window.window.width = 1440
        suite.Window.window.height = 1000
        Dsp.actions = []
        Dsp.viewers = 0
        Dsp.lightRefreshes = 0
        Dsp.fullRefreshes = 0
        Dsp.snapshot = ""
        Dsp.statusReply = {available: true, active: false, name: "Flat"}
        Dsp.document = JSON.stringify({available: true, active: false,
            selection: {plugin: "test.dsp", preset: "flat", enabled: false},
            preset: {id: "flat", name: "Flat", filters: []},
            presets: [], browser: {presets: [], groups: [], selected_ids: [], total: 0},
            favorites: [], curve: [], graphic_frequencies: [], levels: []})
        Dsp.refresh()
    }
    function cleanup() {
        const loader = findChild(control, "dspDialogLoader")
        if (loader.item) loader.item.close()
        tryCompare(loader, "active", false)
        tryCompare(Dsp, "viewers", 0)
        Dsp.statusReply = null
        Dsp.document = ""
    }
    function openControl() {
        control.clicked()
        const loader = findChild(control, "dspDialogLoader")
        tryVerify(() => loader.item !== null)
        tryCompare(loader.item, "opened", true)
        return loader
    }
    function test_closed_control_refreshes_status_without_a_document_or_panel() {
        const count = Dsp.lightRefreshes
        wait(Motion.refresh + 80)
        verify(Dsp.lightRefreshes > count)
        compare(Dsp.fullRefreshes, 0)
        compare(Dsp.snapshot, "")
        compare(findChild(control, "dspDialogLoader").active, false)
        compare(findChild(control, "dspDialogLoader").item, null)
    }
    function test_unavailable_control_keeps_discovering_dynamic_availability() {
        Dsp.statusReply = {available: false, active: false, name: ""}
        Dsp.refresh()
        verify(!control.visible)
        Dsp.statusReply = {available: true, active: true, name: "New preset"}
        tryCompare(Dsp, "available", true, Motion.refresh + 500)
        compare(Dsp.presetName, "New preset")
        verify(control.visible)
        verify(control.checked)
        compare(Dsp.fullRefreshes, 0)
        compare(Dsp.snapshot, "")
    }
    function test_open_close_reopen_releases_the_document_and_editor_owner() {
        const loader = openControl()
        compare(Dsp.viewers, 1)
        verify(Dsp.fullRefreshes > 0)
        verify(Dsp.snapshot.length > 0)
        loader.item.close()
        tryCompare(loader, "active", false)
        tryCompare(Dsp, "viewers", 0)
        compare(Dsp.snapshot, "")
        compare(Dsp.actions.filter(entry => entry.action === "close").length, 1)
        const full = Dsp.fullRefreshes
        wait(Motion.refresh + 80)
        compare(Dsp.fullRefreshes, full)
        openControl()
        compare(Dsp.viewers, 1)
        verify(Dsp.fullRefreshes > full)
        compare(JSON.parse(Dsp.snapshot).preset.name, "Flat")
    }
    function test_destroying_an_open_owner_releases_its_document_and_browser() {
        const panel = panelFactory.createObject(suite)
        panel.open()
        tryCompare(panel, "opened", true)
        compare(Dsp.viewers, 1)
        panel.destroy()
        tryCompare(Dsp, "viewers", 0)
        compare(Dsp.snapshot, "")
        compare(Dsp.actions.filter(entry => entry.action === "close").length, 1)
        const full = Dsp.fullRefreshes
        Dsp.refresh()
        compare(Dsp.fullRefreshes, full)
    }
    function test_a_closed_owner_cannot_release_another_visible_panel() {
        const closed = panelFactory.createObject(suite)
        const loader = openControl()
        closed.destroy()
        wait(0)
        compare(Dsp.viewers, 1)
        verify(Dsp.snapshot.length > 0)
        verify(loader.item.opened)
    }
}
