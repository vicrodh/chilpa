import QtQuick
import QtTest
import app.chilpa
import "../../qml/Screens"
import "../../qml/Patterns"

TestCase {
    name: "ListenBuffers"
    when: windowShown
    visible: true
    width: 1100
    height: 700
    ListenListScreen { id: page; anchors.fill: parent }
    function descendants(item) {
        let found = [item]
        for (const child of item.children || []) found = found.concat(descendants(child))
        return found
    }
    function test_add_button_follows_the_last_queue_tab() {
        const add = findChild(page, "newQueue")
        verify(add)
        const tabs = descendants(page).filter(item => item.title === "Other · 2")
        verify(tabs.length > 0)
        const tab = tabs[0]
        const edge = tab.mapToItem(add.parent, tab.width, 0)
        verify(add.x >= edge.x)
        verify(add.x - edge.x < add.width)
    }
    function test_tabs_select_without_playing_and_keep_live_player() {
        const other = descendants(page).find(item => item.title === "Other · 2")
        verify(other)
        ListenList.lastAction = ""
        other.activated()
        compare(ListenList.lastAction, "select")
        compare(ListenList.lastId, "2")
        compare(JSON.parse(ListenList.snapshot).active, "1")
        compare(Player.path, "/test.wav")
    }
    function tab(title) { return descendants(page).find(item => item.title === title && item.renamable !== undefined) }
    function test_a_queue_is_renamed_on_its_tab_by_its_pencil() {
        ListenList.command("select", "2", "")
        const other = tab("Other · 2")
        tryCompare(other, "active", true)
        const pencil = findChild(other, "renameTab")
        verify(pencil.visible, "the active tab has its pencil")
        verify(!findChild(tab("Jazz · 3"), "renameTab").visible, "an inactive one has none")
        waitForRendering(pencil)
        mouseClick(pencil, pencil.width / 2, pencil.height / 2)
        verify(other.editing, "one click on the pencil")
        const field = findChild(other, "tabName")
        compare(field.text, "Other")
        field.text = "Road trip"
        keyClick(Qt.Key_Return)
        compare(other.editing, false)
        compare(ListenList.renamedId, "2")
        compare(ListenList.renamedText, "Road trip")
    }
    function test_a_double_click_on_the_name_edits_it_and_escape_keeps_the_old_one() {
        ListenList.renamedId = ""
        const main = tab("Jazz · 3")
        waitForRendering(main)
        mouseDoubleClickSequence(main, main.width / 3, main.height / 2)
        verify(main.editing, "a double click on the name")
        keyClick(Qt.Key_X)
        keyClick(Qt.Key_Escape)
        compare(main.editing, false)
        compare(ListenList.renamedId, "", "escape renames nothing")
    }
    function test_save_commits_the_name_being_edited_first() {
        ListenList.command("select", "2", "")
        const other = tab("Other · 2")
        other.rename()
        findChild(other, "tabName").text = "Saved"
        descendants(page).find(item => item.text === "Save as playlist" && item.clicked).clicked()
        compare(ListenList.renamedId, "2")
        compare(ListenList.renamedText, "Saved")
        compare(ListenList.lastAction, "save")
    }
    function test_no_name_field_is_left_on_the_screen() {
        verify(!descendants(page).some(item => item.placeholder === "Queue name"))
    }
    // The queue shows its stars, has the sleep timer, and says how long it
    // lasts and at what time it ends.
    function test_the_queue_rates_has_its_timer_and_says_when_it_ends() {
        verify(descendants(page).find(item => item.showRating !== undefined).showRating)
        page.now = Date.parse("2026-10-10T21:00:00")
        page.foot = JSON.parse(ListenList.footer())
        compare(findChild(page, "queuesSummary").text, "3 songs 1 hr 2 min")
        compare(findChild(page, "queuesEnds").text, "Ends at 21:10")
        findChild(page, "queuesTimer").clicked()
        tryVerify(() => findChild(page, "sleepTimer").opened)
        findChild(page, "sleepTimer").close()
    }
    function test_layout_stays_inside_at_small_and_large_sizes() {
        for (const size of [[1100,700],[720,500]]) {
            page.width = size[0]; page.height = size[1]
            waitForRendering(page)
            const view = findChild(page, "trackListView")
            verify(view.width > 0 && view.height > 0)
            const bottom = view.mapToItem(page, view.width, view.height)
            verify(bottom.x <= page.width + 1)
            verify(bottom.y <= page.height + 1)
        }
    }
}
