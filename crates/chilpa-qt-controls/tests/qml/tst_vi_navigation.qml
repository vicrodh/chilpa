import QtQuick
import QtQuick.Controls.Basic
import QtTest
import app.chilpa
import app.chilpa.controls
import "../../qml/Patterns"
import "../../qml/Components"
import "../../../chilpa-qt/qml/Components"

TestCase {
    id: test
    name: "ViNavigation"
    when: windowShown
    visible: true
    width: 1000
    height: 600
    WindowControls { id: controls; window: test.Window.window }
    TrackListModel { id: tracks }
    TrackList { id: list; width: 1000; height: 400; model: tracks; rich: false }
    TextInput { id: input; y: 430; width: 300; height: 30 }
    Item { id: searchHost; property bool compact: false }
    QtObject { id: query; property string text: ""; property string source: "" }
    Popup {
        id: results
        property bool ownInput: false
        property int current: 0
        property int confirms: 0
        function step(by) { current += by }
        function chooseCurrent() { ++confirms; close() }
        focus: false
    }
    SearchCommands { host: searchHost; search: query; field: input; flyout: results }
    function event(action, count) {
        Hotkeys.answer = action
        Hotkeys.repeat = count || 1
        controls.pressed({ key: Qt.Key_J, modifiers: 0, text: "j", accepted: false })
    }
    function init() {
        Hotkeys.snapshot = JSON.stringify({ keymap: "vi" })
        Hotkeys.searchActive = false
        Hotkeys.repeat = 1
        Hotkeys.answer = ""
        controls.immersiveOpen = false
        results.close()
        results.current = 0
        results.confirms = 0
        input.text = ""
        tracks.clear()
        for (let i = 0; i < 30; ++i)
            tracks.append({trackId:i+1, rowKind: i === 3 ? 1 : 0, number:i+1, title:"Track " + i, artist:"Artist", album:"Album", sourceName:"", duration:"3:00", plays:0, quality:"FLAC", tier:2, available:true, playing:false, selected:false, headerLabel:i===3?"Header":"", headerMeta:"", art:""})
        list.rows.forceLayout()
        list.cursorRow = -1
        list.forceActiveFocus()
    }
    function cleanup() { results.close(); Hotkeys.answer = ""; Hotkeys.searchActive = false }
    function test_cursor_skips_headers_and_runs_real_row_methods() {
        event("list.first")
        compare(list.cursorRow, 0)
        event("list.down", 5)
        compare(list.cursorRow, 6)
        event("list.last")
        compare(list.cursorRow, 29)
        event("list.halfUp")
        verify(list.cursorRow < 29)
        event("list.queue")
        compare(tracks.lastAction, "queue")
        compare(tracks.lastRow, list.cursorRow)
        event("list.favorite")
        compare(tracks.lastAction, "favorite")
        event("list.play")
        compare(tracks.lastAction, "play")
    }
    function test_text_fields_and_immersive_views_keep_background_lists_idle() {
        event("list.first")
        input.forceActiveFocus()
        keyClick("j")
        compare(input.text, "j")
        compare(list.cursorRow, 0)
        controls.pressed({key:Qt.Key_Escape, modifiers:0, text:"", accepted:false})
        verify(list.activeFocus)
        controls.immersiveOpen = true
        event("list.down")
        compare(list.cursorRow, 0)
        event("list.play")
        verify(tracks.lastAction !== "play")
    }
    function test_search_enter_confirms_the_result_above_the_background_list() {
        Hotkeys.searchCommand("Album", "local")
        compare(query.text, "Album")
        compare(query.source, "local")
        verify(Hotkeys.searchActive)
        event("search.next")
        compare(results.current, 1)
        const previous = tracks.lastAction
        event("list.play")
        compare(results.confirms, 1)
        compare(tracks.lastAction, previous)
        verify(!Hotkeys.searchActive)
    }
}
