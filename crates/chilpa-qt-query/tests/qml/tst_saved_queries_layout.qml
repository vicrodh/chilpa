import QtQuick
import QtQuick.Window
import QtQuick.Controls.Basic as Controls
import QtTest
import app.chilpa
import app.chilpa.query
import app.chilpa.kit
import "../../qml/Components"
import "../../qml/Patterns"
import "../../qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

TestCase {
    id: test
    name: "SavedQueriesLayout"
    when: windowShown
    visible: true
    width: 1920; height: 1080
    property bool captureVisuals: false // I enable local review captures without requiring this directory on other machines.
    FontLoader { id: inter; source: "qrc:/qt/qml/app/chilpa/kit/assets/fonts/Inter.ttf" }
    Rectangle { anchors.fill: parent; color: Theme.color.surfaceWindow; z: -1 }
    RefineHead { id: head; x: 12; y: 12; width: 1000; height: implicitHeight; conditions: 2; canSave: true; summary: "23,216 songs · 1625 h" }
    SavedQueriesDialog { id: form; parent: test; font.family: inter.name }
    function initTestCase() {
        tryCompare(inter, "status", FontLoader.Ready)
        // I give the mock a readable neutral palette; sizes and Inter are shipped assets.
        ThemeModel.palette = {"surface.window":"#181818", "surface.raised":"#282828", "surface.pane":"#202020", "surface.sidebar":"#202020",
            "text.primary":"#eeeeee", "text.secondary":"#bbbbbb", "text.tertiary":"#888888", "stroke.hairline":"#444444", "state.hover":"#383838",
            "accent":"#d8323a", "text.onAccent":"#ffffff", "status.negative":"#ff7777"}
        ThemeModel.rev++
    }
    function init() {
        SavedQueries.savedEntries = []
        SavedQueries.catalog = '{"entries":[],"revision":0}'
        SavedQueries.savedError = ""
        SavedQueries.rev++
        form.error = ""
        form.readFailed = false
        form.mode = "list"
        form.detached = false
    }
    function cleanup() { form.close(); tryCompare(form, "opened", false) }
    function catalog(count) {
        const entries = []
        for (let at = 0; at < count; at++) entries.push({id:"q" + (at + 1), name:"Evening collection " + (at + 1), expression:'(genre:jazz | genre:soul) year:1960..1989 -title:"Live"'})
        SavedQueries.savedEntries = entries
        SavedQueries.publishSaved()
    }
    function room(width, height) {
        test.width = width; test.height = height
        test.Window.window.width = width; test.Window.window.height = height
    }
    function point(item, x, y) { return item.mapToItem(form.contentItem, x, y) }
    function geometry() {
        const table = findChild(form, "savedQueriesTable"), list = findChild(form, "savedQueriesList")
        const cancel = findChild(form, "savedQueryCancel"), bar = findChild(form, "savedQueriesScrollBar")
        verify(table.height > 0 && list.height > 0)
        verify(point(table, 0, table.height).y <= point(cancel, 0, 0).y)
        verify(point(table, 0, 0).y >= 0)
        verify(point(cancel, cancel.width, cancel.height).y <= form.contentItem.height)
        verify(list.height <= table.height)
        tryVerify(() => bar.parent === table)
        verify(bar.x >= list.x + list.width, "the scrollbar occupies its own gutter outside the rows")
        verify(bar.x + bar.width <= table.width)
        return list
    }
    function test_detached_uses_a_window_and_main_uses_an_item_popup() {
        compare(form.popupType, Controls.Popup.Item)
        form.detached = true
        compare(form.popupType, Controls.Popup.Window)
        form.detached = false
        compare(form.popupType, Controls.Popup.Item)
    }
    function test_header_actions_share_one_row_without_overlap() {
        for (const width of [520, 1000]) {
            head.width = width
            const count = findChild(head, "refineCount"), save = findChild(head, "refineSaveQuery")
            const open = findChild(head, "refineSavedQueries"), consoleButton = findChild(head, "refineConsoleToggle")
            const match = findChild(head, "refineMatch")
            tryVerify(() => save.width > 0 && open.width > 0)
            compare(save.glyph, Lucide.save); compare(open.glyph, Lucide.fileUp)
            compare(save.iconFamily, Lucide.font); compare(open.iconFamily, Lucide.font)
            const position = item => item.mapToItem(head, 0, 0)
            tryVerify(() => position(match).x + match.width <= position(count).x, 1000, "header width=" + width + " matchRight=" + (position(match).x + match.width) + " countLeft=" + position(count).x)
            verify(position(count).x + count.width <= position(save).x)
            verify(position(save).x + save.width <= position(open).x)
            verify(position(open).x + open.width <= position(consoleButton).x)
            for (const item of [count, save, open, consoleButton]) {
                verify(Math.abs(position(item).y + item.height / 2 - head.height / 2) < 1)
                verify(position(item).x >= 0 && position(item).x + item.width <= width)
            }
            const number = count.children[0], duration = count.children[1]
            compare(number.mapToItem(head, 0, number.baselineOffset).y,
                duration.mapToItem(head, 0, duration.baselineOffset).y)
            compare(head.height, head.mainHeight, "saving adds no second header row")
        }
    }
    function test_catalog_growth_forms_and_scroll_stay_inside_fixed_modal() {
        for (const size of [[960,600], [1920,1080]]) {
            room(size[0], size[1]); catalog(0)
            form.open(); tryCompare(form, "opened", true)
            const width = form.width, height = form.height
            compare(width, Math.min(size[0] - Refine.savedDialogAround, Math.max(Refine.savedDialogMinWide, size[0] * 0.6)))
            compare(height, Math.min(size[1] - Refine.savedDialogAround, Math.max(Refine.savedDialogMinTall, size[1] * 0.6)))
            geometry(); catalog(64)
            const list = geometry(), bar = findChild(form, "savedQueriesScrollBar")
            tryCompare(list, "count", 64)
            compare(form.width, width); compare(form.height, height)
            mouseMove(list, list.width / 2, list.height / 2)
            tryCompare(bar, "visible", true)
            verify(bar.contentItem.width > 0 && bar.contentItem.height > 0 && bar.contentItem.opacity > 0)
            list.positionViewAtEnd(); tryVerify(() => list.contentY > 0)
            compare(form.height, height)
            list.positionViewAtBeginning()
            waitForRendering(form.contentItem)
            if (captureVisuals) grabImage(test.Window.window.contentItem).save("/mnt/build/tmp/blitzkriegfc/refine-visual/catalog-" + size[0] + ".png")
            form.rename(form.entries[0]); geometry()
            const name = findChild(form, "savedQueryName"), submit = findChild(form, "savedQuerySubmit")
            verify(name.visible && submit.visible && submit.enabled)
            verify(point(name, 0, name.height).y < point(submit, 0, 0).y)
            SavedQueries.savedError = "The saved queries could not be saved: disk full."
            name.text = "Keep this name"; form.submit()
            compare(name.text, "Keep this name")
            verify(findChild(form, "savedQueryError").visible)
            geometry(); compare(form.height, height)
            form.mode = "save"; geometry()
            verify(name.visible && submit.visible && submit.enabled)
            waitForRendering(form.contentItem)
            if (captureVisuals) grabImage(test.Window.window.contentItem).save("/mnt/build/tmp/blitzkriegfc/refine-visual/error-form-" + size[0] + ".png")
            form.close(); tryCompare(form, "opened", false)
            SavedQueries.savedError = ""
            form.mode = "list"; form.error = ""
        }
    }
}
