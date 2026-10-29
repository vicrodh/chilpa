import QtQuick
import QtQuick.Controls.Basic
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/playlists/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/controls/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/controls/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"

ApplicationWindow {
    id: window
    width: 1200
    height: 700
    visible: true
    property int fixtureAlbum: 0
    WindowControls { id: controls; window: window }
    Item { id: origin; width: 10; height: 10 }
    Item { id: searchHost; property bool compact: false }
    TextInput { id: field; y: 650; width: 300; height: 30 }
    SearchModel { id: search }
    SearchFlyout { id: results; search: search; room: 600 }
    SearchCommands { host: searchHost; search: search; field: field; flyout: results }
    PlaylistPickerModal { id: playlistPicker }
    FontMetrics { id: commandFont; font: tests.commandBar.input.font }

    TestCase {
        id: tests
        name: "NativeKeyboardControls"
        when: window.visible
        readonly property var commandBar: findChild(controls, "commandBar")
        readonly property var sheet: findChild(controls, "shortcutsSheet")
        readonly property var editor: findChild(controls, "shortcutsEditor")

        function check(value, message) {
            if (!value) console.log("SMOKE FAIL " + qtest_results.functionName + ": " + (message || "verification"))
            verify(value, message)
        }
        function same(actual, expected, message) {
            if (actual !== expected) console.log("SMOKE FAIL " + qtest_results.functionName + ": " + actual + " != " + expected)
            compare(actual, expected, message)
        }
        function descendants(root, predicate) {
            const matches = []
            function visit(item) {
                if (predicate(item)) matches.push(item)
                for (const child of item.children || []) visit(child)
            }
            visit(root)
            return matches
        }
        function keycap(action) {
            return descendants(editor.contentItem, item => item.objectName === "keycap").find(cap => {
                for (let item = cap.parent; item; item = item.parent)
                    if (item.entry !== undefined) return item.entry.id === action
                return false
            })
        }
        function reveal(cap) {
            let viewport = cap.parent
            while (viewport && viewport.contentY === undefined) viewport = viewport.parent
            check(!!viewport, "the shortcut belongs to a scrollable list")
            viewport.contentY = Math.max(0, Math.min(viewport.contentHeight - viewport.height,
                cap.mapToItem(viewport.contentItem, 0, 0).y - viewport.height / 2))
            waitForRendering(cap)
            const at = cap.mapToItem(viewport, 0, 0)
            check(at.y >= 0 && at.y + cap.height <= viewport.height, "the shortcut must be inside the viewport before clicking")
        }
        function shown(popup) {
            tryCompare(popup, "opened", true)
            waitForRendering(popup.contentItem)
        }
        function write(text) { for (const character of text) keyClick(character) }
        function init() {
            console.log("SMOKE native keyboard begin " + qtest_results.functionName)
            try {
                Hotkeys.command("cancel", "")
                Hotkeys.command("reset-all", "")
                Hotkeys.command("keymap", "default")
                Navigation.go("library", 0)
                search.text = ""
                field.text = ""
                results.current = -1
                origin.forceActiveFocus()
            } catch (error) {
                console.log("SMOKE FAIL native keyboard init " + error)
                throw error
            }
        }
        function cleanup() {
            console.log("SMOKE native keyboard result " + qtest_results.functionName + " failed=" + qtest_results.failed + " bar=" + commandBar)
            results.close()
            playlistPicker.close()
            if (typeof search.cancelSelection === "function") search.cancelSelection()
            editor.close()
            sheet.close()
            if (commandBar.opened) commandBar.dismiss()
            check(!Player.playing || Player.paused, "keyboard navigation must keep the restored session silent")
        }
        function cleanupTestCase() {
            if (qtest_results.failCount > 0) {
                console.log("SMOKE FAIL native keyboard assertions=" + qtest_results.failCount)
                Qt.exit(1)
            } else {
                console.log("SMOKE native keyboard completed")
                Qt.exit(0)
            }
        }
        function test_colon_opens_in_every_keymap_data() {
            return [{tag:"default", keymap:"default"}, {tag:"letters", keymap:"letters"}, {tag:"vi", keymap:"vi"}]
        }
        function test_colon_opens_in_every_keymap(data) {
            Hotkeys.command("keymap", data.keymap)
            keyClick(Qt.Key_Colon, Qt.ShiftModifier)
            check(commandBar.opened, "colon must open the console in " + data.keymap)
            check(commandBar.input.activeFocus)
            same(commandBar.input.text, "")
            keyClick(Qt.Key_Escape)
            check(!commandBar.opened)
            check(origin.activeFocus)
        }
        function test_editor_rebinds_with_real_control_and_shift_keys() {
            keyClick(Qt.Key_Question, Qt.ShiftModifier)
            shown(sheet)
            mouseClick(findChild(sheet.contentItem, "customize"))
            shown(editor)
            let cap = keycap("commands.open")
            check(cap !== undefined)
            reveal(cap)
            mouseClick(cap)
            tryCompare(editor, "recording", true)
            keyClick(Qt.Key_Semicolon, Qt.ControlModifier)
            tryCompare(editor, "recording", false)
            same(Hotkeys.shortcut("commands.open"), "Ctrl + ;")
            cap = keycap("commands.open")
            check(cap !== undefined)
            reveal(cap)
            mouseClick(cap)
            tryCompare(editor, "recording", true)
            keyClick(Qt.Key_Colon, Qt.ShiftModifier)
            tryCompare(editor, "recording", false)
            same(Hotkeys.shortcut("commands.open"), ":")
            editor.close()
            tryCompare(editor, "opened", false)
            origin.forceActiveFocus()
            keyClick(Qt.Key_Colon, Qt.ShiftModifier)
            check(commandBar.opened && commandBar.input.activeFocus)
        }
        function test_guide_has_only_bound_or_command_rows_and_a_scrollbar_gutter() {
            keyClick(Qt.Key_Question, Qt.ShiftModifier)
            shown(sheet)
            const caps = descendants(sheet.contentItem, item => item.objectName === "keycap" && item.visible)
            check(caps.length > 10)
            check(caps.every(cap => cap.text.trim() !== ""), "the guide must not draw empty badges")
            const rows = descendants(sheet.contentItem, item => item.objectName === "shortcutRow")
            check(rows.length > 10)
            check(rows.every(row => (row.entry.shortcut || "") !== "" || (row.entry.command || "") !== ""))
            const groups = descendants(sheet.contentItem, item => item.objectName === "shortcutGroup")
            check(groups.length > 0 && groups.every(group => group.modelData.rows.length > 0))
            const columns = findChild(sheet.contentItem, "shortcutColumns")
            const bar = findChild(sheet.contentItem, "shortcutScrollBar")
            check(columns !== null && bar !== null)
            check(bar.x >= columns.width)
            check(bar.x + bar.width <= columns.parent.width)
        }
        function test_command_font_is_fixed_width() {
            keyClick(Qt.Key_Colon, Qt.ShiftModifier)
            check(commandBar.opened)
            check(commandBar.input.font.family.trim() !== "")
            check(Math.abs(commandFont.advanceWidth("iiii") - commandFont.advanceWidth("WWWW")) < 0.1)
        }
        function openSearch(opener) {
            if (opener === "slash") Hotkeys.command("keymap", "vi")
            keyClick(opener === "colon" ? Qt.Key_Colon : Qt.Key_Slash, opener === "colon" ? Qt.ShiftModifier : Qt.NoModifier)
            check(commandBar.opened && commandBar.input.activeFocus)
            if (opener === "slash") {
                same(commandBar.input.text, "search ")
                keyClick(Qt.Key_Backspace)
            }
            write(opener === "colon" ? "search:local Vi smoke fixture" : ":local Vi smoke fixture")
            keyClick(Qt.Key_Return)
            shown(results)
            tryVerify(() => search.count > 2)
            waitForRendering(results.contentItem)
            check(!commandBar.opened)
            same(search.text, "Vi smoke fixture")
            same(search.source, "local")
            check(Hotkeys.searchActive)
        }
        function test_search_arrows_and_enter_open_the_native_album_data() {
            return [{tag:"colon", opener:"colon"}, {tag:"slash", opener:"slash"}]
        }
        function test_search_arrows_and_enter_open_the_native_album(data) {
            openSearch(data.opener)
            keyClick(Qt.Key_Down)
            const albumRow = results.current
            const albums = descendants(results.contentItem, item => item.kind === 3 && item.title === "Vi smoke fixture")
            check(albums.some(item => item.index === albumRow), "the first selected native result must be the fixture album")
            keyClick(Qt.Key_Down)
            check(results.current > albumRow)
            keyClick(Qt.Key_Up)
            same(results.current, albumRow)
            keyClick(Qt.Key_Return)
            tryCompare(results, "opened", false)
            tryCompare(Navigation, "screen", "album")
            same(Navigation.routeId, window.fixtureAlbum)
            check(!Player.playing || Player.paused)
        }
        function test_search_escape_restores_origin_and_preserves_query() {
            openSearch("colon")
            keyClick(Qt.Key_Down)
            check(results.current >= 0)
            keyClick(Qt.Key_Escape)
            tryCompare(results, "opened", false)
            check(origin.activeFocus)
            same(search.text, "Vi smoke fixture")
            same(Navigation.screen, "library")
        }
        function selectedAlbumMenu() {
            openSearch("colon")
            keyClick(Qt.Key_Down)
            const menu = findChild(results, "searchResultMenu")
            check(menu !== null, "the native search menu must be mounted")
            menu.show(results.current, 0, 0)
            shown(menu)
            return menu
        }
        function test_menu_keeps_the_selected_album_after_results_are_rebuilt() {
            const menu = selectedAlbumMenu()
            const open = menu.entries.findIndex(entry => entry.act === "open")
            check(open >= 0)
            search.text = "Vi fixture track"
            tryVerify(() => search.count > 0)
            search.refresh()
            tryVerify(() => descendants(results.contentItem, item => item.kind === 4 && item.title.startsWith("Vi fixture track")).length > 0)
            menu.choose(open, 0)
            tryCompare(Navigation, "screen", "album")
            same(Navigation.routeId, window.fixtureAlbum, "menu destination must survive a model reset")
            check(!Player.playing || Player.paused)
        }
        function test_menu_reads_the_album_before_opening_the_playlist_picker() {
            const menu = selectedAlbumMenu()
            const pick = menu.entries.findIndex(entry => entry.act === "pick")
            check(pick >= 0)
            menu.choose(pick, 0)
            shown(playlistPicker)
            same(JSON.parse(playlistPicker.trackIdsJson).length, 100, "the picker receives every real album track")
            check(!search.selectionPending)
            same(search.selectionError, "")
            keyClick(Qt.Key_Escape)
            tryCompare(playlistPicker, "opened", false)
            check(!Player.playing || Player.paused)
        }
        function test_search_reopens_with_native_n_and_keeps_keyboard_navigation() {
            openSearch("colon")
            keyClick(Qt.Key_Down)
            const albumRow = results.current
            keyClick(Qt.Key_Escape)
            tryCompare(results, "opened", false)
            check(origin.activeFocus)
            Hotkeys.command("keymap", "vi")
            keyClick(Qt.Key_N)
            shown(results)
            const reopened = results.current
            check(reopened > albumRow)
            keyClick(Qt.Key_N, Qt.ShiftModifier)
            same(results.current, albumRow, "N must reach the focused popup")
            keyClick(Qt.Key_N)
            same(results.current, reopened, "n must keep stepping while the popup is open")
            keyClick(Qt.Key_K)
            same(results.current, albumRow, "k moves within the focused results")
            keyClick(Qt.Key_J)
            same(results.current, reopened, "j moves within the focused results")
            keyClick(Qt.Key_Down)
            check(results.current > reopened, "reopened search must receive Down")
            keyClick(Qt.Key_Up)
            same(results.current, reopened)
            keyClick(Qt.Key_Up)
            same(results.current, albumRow)
            keyClick(Qt.Key_Return)
            tryCompare(results, "opened", false)
            tryCompare(Navigation, "screen", "album")
            same(Navigation.routeId, window.fixtureAlbum)
        }
    }
    Timer {
        interval: 60000
        running: true
        onTriggered: { console.log("SMOKE FAIL native keyboard timed out"); Qt.exit(1) }
    }
}
