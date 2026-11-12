import QtQuick
import QtQuick.Window
import QtTest
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../../qml/Theme"
import "qrc:/qt/qml/app/chilpa/qml/Screens" as Screens

TestCase {
    id: test
    name: "TrackSheet"
    when: windowShown
    visible: true
    width: 1100
    height: 800
    Component { id: screenMaker; Screens.TracksScreen {} }
    TrackColumns { id: columns; width: 1100; sheet: true }
    TrackSheetHeader { id: header; width: columns.width; cols: columns }
    Item { id: anchor; x: 1060; y: 720; width: 30; height: 30 }
    QtObject {
        id: backend
        property string columnResult: ""
        property bool columnBusy: false
        property var requests: []
        property int closes: 0
        function requestColumn(field, search, cursor) { requests = requests.concat([{field, search, cursor}]) }
        function closeColumn() { closes++ }
        function answer(items, cursor, next, more) {
            const request = requests[requests.length - 1]
            columnResult = JSON.stringify({field: request.field, search: request.search, cursor, items, next, more})
        }
    }
    TrackColumnFilter { id: popup; model: backend }
    SignalSpy { id: requested; target: header; signalName: "requested" }
    SignalSpy { id: applied; target: popup; signalName: "applied" }
    function init() {
        failOnWarning(/ReferenceError|TypeError|Binding loop detected/)
        columns.optionalColumns = []
        columns.rich = false
        Prefs.ratings = true
        backend.requests = []
        applied.clear()
        requested.clear()
    }
    function cleanup() { popup.close(); wait(1) }
    function test_only_the_three_text_columns_are_required() {
        popup.field = "format"
        compare(popup.display("", ""), "Unknown")
        verify(columns.titleW > 0 && columns.artistW > 0 && columns.albumW > 0)
        for (const name of ["art", "sourceWidth", "quality", "format", "rating", "plays", "time", "favorite"])
            compare(columns[name], 0, name)
        columns.rich = true
        verify(columns.titleW > 0 && columns.artistW > 0 && columns.albumW > 0)
        compare(columns.art, 0)
        columns.optionalColumns = ["art", "source", "quality", "format", "rating", "plays", "time", "favorite"]
        columns.showSource = true
        columns.showPlays = true
        for (const name of ["art", "sourceWidth", "quality", "format", "rating", "plays", "time", "favorite"])
            verify(columns[name] > 0, name)
        compare(findChild(header, "tracksColumn-artist").x, columns.artistX)
        compare(findChild(header, "tracksColumn-format").x, columns.formatX)
    }
    function test_horizontal_clipping_releases_the_clocks_on_the_same_row() {
        const page = createTemporaryObject(screenMaker, test, {width:620, height:500})
        page.shownColumns = ["index", "source", "time", "rating", "quality", "favorite"]
        const model = page.scrollView.model
        model.append({trackId:1,rowKind:0,number:1,title:"Track",artist:"Artist",album:"Album",duration:"3:00",plays:0,
            quality:"FLAC",tier:2,available:true,playing:true,selected:false,headerLabel:"",headerMeta:"",art:""})
        Prefs.animatedIndicator = true
        Player.playing = true; Player.paused = false
        Ratings.pendingTracks = {"1":true}; Ratings.rev++
        page.scrollView.forceLayout()
        const sheet = findChild(page, "tracksSheet")
        const row = page.scrollView.itemAtIndex(0)
        verify(row !== null)
        tryCompare(Pulse, "held", 1, 1000, "only the visible playing mark runs")
        sheet.contentX = sheet.contentWidth - sheet.width
        compare(page.scrollView.itemAtIndex(0), row)
        tryCompare(Pulse, "held", 1, 1000, "the heart runs while the playing mark is clipped")
        Ratings.pendingTracks = ({}); Ratings.rev++
        tryCompare(Pulse, "held", 0)
        sheet.contentX = 0
        tryCompare(Pulse, "held", 1)
        Prefs.animatedIndicator = false
        Player.playing = false
        tryCompare(Pulse, "held", 0)
    }
    function test_keyboard_opens_headers_and_selects_checklist_values() {
        const title = findChild(header, "tracksColumn-title")
        title.forceActiveFocus()
        keyClick(Qt.Key_Return)
        compare(requested.count, 1)
        popup.show("title", "Title", [], anchor)
        backend.answer([{key:"A",label:"A",count:1}], "", "", false)
        const list = findChild(popup.contentItem, "tracksFilterChoices")
        list.forceActiveFocus()
        list.currentIndex = 0
        keyClick(Qt.Key_Space)
        compare(popup.draft, ["A"])
        keyClick(Qt.Key_Return)
        compare(applied.signalArguments[0][1], ["A"])
    }
    function test_sheet_does_not_change_other_lists() {
        columns.sheet = false
        columns.rich = true
        compare(columns.artistW, 0)
        verify(columns.art > 0 && columns.quality > 0 && columns.time > 0)
        columns.sheet = true
    }
    function test_draft_preserves_selections_across_search_and_pages() {
        popup.show("title", "Title", ["Kept"], anchor)
        compare(backend.requests.length, 1)
        backend.answer([{key:"A", label:"A", count:1}], "", "page2", true)
        compare(popup.options.length, 1)
        popup.toggle("A")
        compare(popup.draft, ["Kept", "A"])
        findChild(popup.contentItem, "tracksFilterMore").clicked()
        backend.answer([{key:"B", label:"B", count:2}], "page2", "", false)
        compare(popup.options.length, 1, "only the current page stays resident")
        compare(popup.options[0].key, "B")
        popup.toggle("B")
        popup.apply()
        compare(applied.signalArguments[0][1], ["Kept", "A", "B"])
    }
    function test_search_is_debounced_and_stale_results_are_ignored() {
        popup.show("artist", "Artist", [], anchor)
        const search = findChild(popup.contentItem, "tracksFilterSearch")
        search.text = "One"
        search.text = "Two"
        tryCompare(popup, "searched", "Two")
        compare(backend.requests.length, 2)
        backend.columnResult = JSON.stringify({field:"artist", search:"One", cursor:"", items:[{key:"wrong"}]})
        compare(popup.options.length, 0)
        backend.answer([{key:"right",label:"right",count:1}], "", "", false)
        compare(popup.options[0].key, "right")
        search.text = "Two "
        wait(Motion.searchSettle + 20)
        compare(backend.requests.length, 2, "trailing whitespace does not repeat SQL")
        compare(popup.options[0].key, "right")
    }
    function test_cancel_releases_data_without_applying() {
        popup.show("album", "Album", ["Kept"], anchor)
        popup.toggle("Other")
        const before = backend.closes
        popup.close()
        compare(applied.count, 0)
        compare(popup.options.length, 0)
        compare(popup.draft.length, 0)
        compare(backend.closes, before + 1)
    }
    function test_favorite_has_three_states() {
        popup.show("favorite", "Favorite", false, anchor)
        compare(popup.draft, ["false"])
        popup.toggle("true")
        compare(popup.draft, ["true"])
        popup.apply()
        compare(applied.signalArguments[0][1], true)
        popup.show("favorite", "Favorite", false, anchor)
        popup.draft = []
        popup.apply()
        compare(applied.signalArguments[1][1], null)
    }
    function test_page_filters_are_independent_and_columns_survive_reopening() {
        Library.qualities = ["lossy"]
        const page = createTemporaryObject(screenMaker, test, {width:1000, height:700})
        verify(page)
        page.columnFilters = {artist:["Band"], favorite:false, rating:["5"]}
        const filters = JSON.parse(page.scrollView.model.key.split("\u001f")[4])
        compare(filters.artist, ["Band"])
        compare(filters.favorite, false)
        compare(Library.qualities, ["lossy"])
        Prefs.ratings = false
        compare(JSON.parse(page.scrollView.model.key.split("\u001f")[4]).rating, undefined)
        compare(page.columnFilters.rating, ["5"], "disabling ratings preserves the user's selection")
        page.columnFilters = {rating:["5"]}
        compare(findChild(page, "tracksResetFilters").visible, false, "disabled ratings do not advertise an active filter")
        SettingsModel.keep("ui.tracks.columns", "[]")
        const reopened = createTemporaryObject(screenMaker, test, {width:1000, height:700})
        compare(reopened.shownColumns, [], "an explicit empty list keeps only mandatory columns")
        SettingsModel.keep("ui.tracks.columns", "invalid")
        const recovered = createTemporaryObject(screenMaker, test, {width:1000, height:700})
        verify(recovered.shownColumns.length > 0, "invalid saved settings recover the defaults")
        SettingsModel.keep("ui.tracks.columns", "")
        Library.qualities = []
    }
    function test_popup_fits_near_window_edges() {
        popup.show("title", "Title", [], anchor)
        verify(popup.x >= 0 && popup.x + popup.width <= test.width)
        backend.answer(Array.from({length:100}, (_, i) => ({key:String(i),label:"Value " + i,count:1})), "", "next", true)
        wait(1)
        verify(popup.y >= 0 && popup.y + popup.height <= test.height)
        test.Window.window.height = 360
        tryVerify(() => popup.parent.height <= 360)
        popup.fit()
        tryVerify(() => popup.y >= 0 && popup.y + popup.height <= popup.parent.height)
        test.Window.window.height = 800
    }
}
