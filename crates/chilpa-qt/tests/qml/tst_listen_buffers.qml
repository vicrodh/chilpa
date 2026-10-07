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
    function test_rename_is_committed_to_its_buffer_when_switching_tabs() {
        ListenList.command("select", "1", "")
        const field = descendants(page).find(item => item.placeholder === "Queue name")
        field.focusInput()
        keyClick(Qt.Key_R)
        compare(field.text, "r")
        ListenList.command("select", "2", "")
        compare(ListenList.renamedId, "1")
        compare(ListenList.renamedText, "r")
        compare(field.text, "Other")
    }
    function test_save_commits_the_edited_name_first() {
        ListenList.command("select", "2", "")
        const field = descendants(page).find(item => item.placeholder === "Queue name")
        field.focusInput()
        keyClick(Qt.Key_S)
        descendants(page).find(item => item.text === "Save as playlist" && item.clicked).clicked()
        compare(ListenList.renamedId, "2")
        compare(ListenList.renamedText, "s")
        compare(ListenList.lastAction, "save")
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
