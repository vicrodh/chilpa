import QtQuick
import QtQuick.Window
import QtTest
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/qml/Screens" as Screens

TestCase {
    id: test
    name: "TrackSheet"
    when: windowShown
    visible: true
    width: 1100
    height: 800
    Component { id: screenMaker; Screens.TracksScreen {} }
    Component { id: viewportMaker; Item {} }
    Component {
        id: playerCoverMaker
        Rectangle {
            property int presses: 0
            x: Metrics.spaceWindow
            y: parent.height - height - Metrics.spaceWindow
            width: parent.width - Metrics.spaceWindow * 2
            height: Metrics.playerSpace
            visible: height > 0
            color: "gray"
            z: 10
            MouseArea { anchors.fill: parent; onPressed: parent.presses++ }
        }
    }
    TrackColumns { id: columns; width: 1100; sheet: true }
    TrackSheetHeader { id: header; width: columns.width; cols: columns }
    Item { id: anchor; x: 1060; y: 720; width: 30; height: 30 }
    QtObject {
        id: backend
        property string columnResult: ""
        property bool columnBusy: false
        property var requests: []
        property int closes: 0
        function requestColumn(field, search, cursor) { columnResult = ""; columnBusy = true; requests = requests.concat([{field, search, cursor}]) }
        function closeColumn() { closes++; columnResult = ""; columnBusy = false }
        function answer(items, cursor, next, more) {
            const request = requests[requests.length - 1]
            columnResult = JSON.stringify({field: request.field, search: request.search, cursor, items, next, more})
            columnBusy = false
        }
        function answerPage() {
            const cursor = requests[requests.length - 1].cursor
            const start = cursor === "" ? 0 : Number(cursor)
            answer(Array.from({length:100}, (_, i) => ({key:String(start + i),label:"Value " + (start + i),count:1})), cursor, String(start + 100), start + 100 < 800)
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
        Metrics.playerSpace = Metrics.playerHeight
        Library.busy = false
        backend.requests = []
        applied.clear()
        requested.clear()
    }
    function cleanup() {
        popup.close()
        popup.contentViewport = null
        test.Window.window.height = 800
        anchor.y = 720
        Library.sourceKeys = []; Library.sourceLabels = []
        Prefs.ratings = true
        Library.busy = false
        wait(1)
    }
    function visibleChoice(list) {
        list.forceLayout()
        const index = list.indexAt(Metrics.gap2, list.contentY + Metrics.gap2)
        verify(index >= 0)
        const item = list.itemAtIndex(index)
        verify(item !== null)
        const current = popup.choiceWindow.optionAt(list.currentIndex)
        return {key:popup.choiceWindow.optionAt(index).key, offset:item.y - list.contentY, current:current ? current.key : ""}
    }
    function compareVisible(list, before) {
        const after = visibleChoice(list)
        compare(after.key, before.key, "the same first option stays visible")
        fuzzyCompare(after.offset, before.offset, 0.5, "partial-row offset is preserved")
        compare(after.current, before.current, "keyboard focus follows the same option")
    }
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
        compare(findChild(header, "tracksColumn-artist").x, columns.artistX - Metrics.gap8)
        compare(findChild(header, "tracksColumn-format").x, columns.formatX - Metrics.gap8)
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
        compare(popup.choiceWindow.count, 1)
        popup.toggle("A")
        compare(popup.draft, ["Kept", "A"])
        popup.choiceWindow.loadNext()
        backend.answer([{key:"B", label:"B", count:2}], "page2", "", false)
        compare(popup.choiceWindow.count, 2, "scrolling appends without replacing the first page")
        compare(popup.choiceWindow.optionAt(1).key, "B")
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
        compare(popup.choiceWindow.count, 0)
        backend.answer([{key:"right",label:"right",count:1}], "", "", false)
        compare(popup.choiceWindow.optionAt(0).key, "right")
        search.text = "Two "
        wait(Motion.searchSettle + 20)
        compare(backend.requests.length, 2, "trailing whitespace does not repeat SQL")
        compare(popup.choiceWindow.optionAt(0).key, "right")
    }
    function test_cancel_releases_data_without_applying() {
        popup.show("album", "Album", ["Kept"], anchor)
        popup.toggle("Other")
        const before = backend.closes
        popup.close()
        compare(applied.count, 0)
        compare(popup.choiceWindow.count, 0)
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
        compare(findChild(page, "tracksFilterTrail").visible, false, "disabled ratings do not advertise an active filter")
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
    function test_hidden_filters_have_readable_tags_and_can_be_cleared() {
        Library.sourceKeys = ["provider:account"]
        Library.sourceLabels = ["Listening room"]
        const page = createTemporaryObject(screenMaker, test, {width:1000, height:700})
        page.shownColumns = []
        page.typed = "Evening music"
        page.columnFilters = {artist:["Band"], source:["provider:account"], format:["flac"], quality:["hires"], favorite:false, rating:["0", "5"]}
        const trail = findChild(page, "tracksFilterTrail")
        tryCompare(trail, "visible", true)
        compare(trail.entries.length, 7)
        for (const expected of [["source", "Source", "Listening room"], ["format", "Format", "FLAC"],
            ["quality", "Quality", "Hi-Res"], ["favorite", "Favorite", "No"],
            ["rating", "Rating", "Unrated, 5 / 5"], ["search", "Search", "Evening music"]]) {
            const tag = findChild(trail, "tracksActive-" + expected[0])
            verify(tag !== null)
            compare(tag.label, expected[1])
            compare(tag.value, expected[2])
        }
        Prefs.ratings = false
        verify(trail.entries.every(entry => entry.field !== "rating"))
        compare(page.columnFilters.rating, ["0", "5"])
        Prefs.ratings = true
        verify(trail.entries.some(entry => entry.field === "rating"))
        const favorite = findChild(findChild(trail, "tracksActive-favorite"), "tracksRemoveFilter")
        favorite.forceActiveFocus(); keyClick(Qt.Key_Return)
        compare(page.columnFilters.favorite, undefined)
        compare(page.columnFilters.source, ["provider:account"])
        const search = findChild(findChild(trail, "tracksActive-search"), "tracksRemoveFilter")
        search.forceActiveFocus(); keyClick(Qt.Key_Space)
        compare(page.typed, "")
        compare(page.columnFilters.artist, ["Band"])
        const clear = findChild(trail, "tracksClearFilters")
        mouseClick(clear, clear.width / 2, clear.height / 2)
        compare(Object.keys(page.columnFilters).length, 0)
        compare(page.typed, "")
        compare(trail.visible, false)
    }
    function test_popup_uses_anchor_room_and_compact_actions() {
        anchor.y = 30
        popup.show("title", "Title", [], anchor)
        backend.answer([{key:"One",label:"One",count:1}], "", "", false)
        wait(1)
        verify(popup.below)
        verify(popup.y >= popup.anchorBottom)
        verify(popup.height <= popup.panelRoom + 0.5)
        anchor.y = 720
        popup.fit(); wait(1)
        verify(!popup.below)
        verify(popup.y + popup.height <= popup.anchorTop + 0.5)
        popup.close()
        anchor.y = 360
        popup.show("title", "Title", [], anchor); backend.answerPage()
        wait(1)
        const tall = popup.height
        verify(tall <= popup.parent.height * 0.4 + 0.5, "the entire flyout uses at most forty percent of the viewport")
        verify(popup.y >= popup.margins && popup.y + popup.height <= popup.parent.height - popup.margins + 0.5)
        test.Window.window.height = 360
        tryVerify(() => popup.parent.height <= 360)
        popup.fit(); wait(1)
        verify(popup.height < tall)
        verify(popup.height <= popup.panelRoom + 0.5)
        verify(popup.y >= popup.margins && popup.y + popup.height <= popup.parent.height - popup.margins + 0.5)
        const apply = findChild(popup.contentItem, "tracksFilterApply")
        const cancel = findChild(popup.contentItem, "tracksFilterCancel")
        compare(apply.height, 24)
        compare(cancel.height, 24)
        mouseClick(cancel, cancel.width / 2, cancel.height / 2)
        compare(popup.opened, false)
        compare(applied.count, 0)
    }
    function test_popup_long_lists_keep_actions_and_scroll_within_content_budget_data() {
        const rows = []
        for (const field of ["artist", "album"])
            for (const height of [800, 1350, 360]) rows.push({tag:field + "-" + height, field, height, contentHeight:0})
        rows.push({tag:"artist-smaller-content",field:"artist",height:1350,contentHeight:480})
        rows.push({tag:"album-smaller-content",field:"album",height:800,contentHeight:360})
        return rows
    }
    function test_popup_long_lists_keep_actions_and_scroll_within_content_budget(data) {
        test.Window.window.height = data.height
        if (data.contentHeight) popup.contentViewport = createTemporaryObject(viewportMaker, test, {width:1000,height:data.contentHeight})
        anchor.y = 30
        popup.show(data.field, data.field, [], anchor)
        backend.answerPage()
        const list = findChild(popup.contentItem, "tracksFilterChoices")
        const apply = findChild(popup.contentItem, "tracksFilterApply")
        const cancel = findChild(popup.contentItem, "tracksFilterCancel")
        verify(waitForRendering(list))
        const available = data.contentHeight || popup.parent.height
        verify(popup.height <= available * 0.4 + 0.5, "popup height " + popup.height + " exceeds forty percent of " + available)
        verify(list.height >= Metrics.hitMin, "at least one actionable option remains visible")
        verify(list.contentHeight > list.height)
        compare(apply.height, 24); compare(cancel.height, 24)
        verify(apply.mapToItem(popup.contentItem, 0, 0).y >= 0)
        verify(apply.mapToItem(popup.contentItem, 0, apply.height).y <= popup.contentItem.height + 0.5, "Apply remains inside the panel")
        verify(cancel.mapToItem(popup.contentItem, 0, cancel.height).y <= popup.contentItem.height + 0.5)
        list.positionViewAtEnd(); list.forceLayout()
        mouseMove(list, list.width / 2, list.height / 2)
        mouseWheel(list, list.width / 2, list.height / 2, 0, -120)
        tryCompare(backend, "columnBusy", true)
        compare(backend.requests.length, 2, "the compact viewport can reach the next page")
        backend.answerPage()
        compare(popup.choiceWindow.count, 200)
        mouseClick(cancel, cancel.width / 2, cancel.height / 2)
        compare(popup.opened, false)
    }
    function test_loading_skeleton_precedes_results_and_never_claims_empty() {
        const started = Date.now()
        let page = createTemporaryObject(screenMaker, test, {width:1000,height:700})
        let model = page.scrollView.model
        model.ready = false
        let skeleton = findChild(page, "tracksLoadingSkeleton")
        let empty = findChild(page, "tracksEmptyState")
        compare(empty.visible, false)
        tryVerify(() => skeleton.active && skeleton.item !== null, 450)
        verify(skeleton.item.count > 0 && skeleton.item.count <= Metrics.trackSkeletonMaxRows)
        verify(waitForRendering(skeleton.item, 450))
        verify(Date.now() - started < 500, "the loading surface renders before the half-second deadline")
        wait(650)
        compare(empty.visible, false)
        verify(skeleton.active && skeleton.item !== null)
        model.append({trackId:1,rowKind:0,number:1,title:"Arrived",artist:"Artist",album:"Album",duration:"3:00",plays:0,
            quality:"FLAC",tier:2,available:true,playing:false,selected:false,headerLabel:"",headerMeta:"",art:""})
        tryCompare(skeleton, "active", false)
        compare(skeleton.item, null)
        compare(empty.visible, false)
        model.ready = true
        model.clear()
        tryCompare(empty, "visible", true, 500, "only a completed empty query displays the empty state")
        Library.busy = true
        compare(empty.visible, false)
        tryVerify(() => skeleton.active && skeleton.item !== null)
        wait(650)
        compare(empty.visible, false)
        Library.busy = false
        compare(empty.visible, true)
        page.destroy(); wait(1)
        const returning = Date.now()
        page = createTemporaryObject(screenMaker, test, {width:1000,height:700})
        model = page.scrollView.model; model.ready = false
        skeleton = findChild(page, "tracksLoadingSkeleton")
        empty = findChild(page, "tracksEmptyState")
        compare(empty.visible, false)
        tryVerify(() => skeleton.item !== null, 450)
        verify(waitForRendering(skeleton.item, 450))
        verify(Date.now() - returning < 500, "re-entering Tracks restores its loading surface immediately")
    }
    function test_marks_only_refresh_queries_that_filter_marks() {
        const page = createTemporaryObject(screenMaker, test, {width:1000,height:700})
        const model = page.scrollView.model
        for (const filters of [{}, {artist:["Band"]}]) {
            page.columnFilters = filters
            const before = model.filterRev
            Ratings.rev++
            compare(model.filterRev, before, "unfiltered marks do not resubmit Tracks")
        }
        for (const filters of [{rating:["5"]}, {favorite:false}, {favorite:true}]) {
            page.columnFilters = filters
            const before = model.filterRev
            Ratings.rev++
            compare(model.filterRev, before + 1, "active marks filters follow their revision")
        }
        page.columnFilters = {rating:["5"]}
        Prefs.ratings = false
        const before = model.filterRev
        Ratings.rev++
        compare(model.filterRev, before, "disabled ratings do not resubmit Tracks")
    }
    function test_global_clear_cancels_the_pending_search() {
        const page = createTemporaryObject(screenMaker, test, {width:1000, height:700})
        page.typed = ""
        page.columnFilters = {artist:["Band"]}
        const tools = page.children.find(child => typeof child.clearSearch === "function")
        verify(tools !== undefined)
        tools.flexible.text = "Pending search"
        compare(page.typed, "", "the search has not settled yet")
        const clear = findChild(page, "tracksClearFilters")
        mouseClick(clear, clear.width / 2, clear.height / 2)
        compare(tools.flexible.text, "")
        wait(Motion.searchSettle + 20)
        compare(page.typed, "")
        compare(Object.keys(page.columnFilters).length, 0)
        compare(findChild(page, "tracksFilterTrail").visible, false)
    }
    function test_scrolling_pages_preserves_visible_options_and_keyboard_focus() {
        popup.show("title", "Title", ["Kept"], anchor); backend.answerPage()
        const list = findChild(popup.contentItem, "tracksFilterChoices")
        list.forceActiveFocus()
        for (let page = 1; page < 3; ++page) {
            list.positionViewAtEnd(); list.forceLayout()
            const before = backend.requests.length
            list.prefetch(-1)
            compare(backend.requests.length, before, "opposite direction does not fetch")
            list.prefetch(1)
            compare(backend.requests.length, before + 1)
            backend.answerPage()
            wait(1)
            compare(backend.requests.length, before + 1, "a response does not fetch another page")
        }
        compare(popup.choiceWindow.count, 300)
        list.positionViewAtEnd(); list.contentY -= 7; list.forceLayout()
        list.currentIndex = list.indexAt(Metrics.gap2, list.contentY + Metrics.gap2) + 2
        const beforeTrim = visibleChoice(list)
        popup.toggle(beforeTrim.current)
        list.prefetch(1); backend.answerPage()
        wait(1)
        compare(popup.choiceWindow.count, 300)
        compare(popup.choiceWindow.optionAt(0).key, "100")
        compareVisible(list, beforeTrim)
        verify(popup.draft.indexOf(beforeTrim.current) >= 0)
        list.positionViewAtBeginning(); list.contentY += 7; list.forceLayout()
        list.currentIndex = 2
        const beforePrepend = visibleChoice(list)
        popup.toggle(beforePrepend.current)
        const before = backend.requests.length
        list.prefetch(1)
        compare(backend.requests.length, before)
        list.prefetch(-1); backend.answerPage()
        wait(1)
        compare(popup.choiceWindow.optionAt(0).key, "0")
        compareVisible(list, beforePrepend)
        verify(popup.draft.indexOf(beforeTrim.current) >= 0)
        verify(popup.draft.indexOf(beforePrepend.current) >= 0)
        keyClick(Qt.Key_Space)
        verify(popup.draft.indexOf(beforePrepend.current) < 0, "space toggles the preserved focused option")
        const idle = backend.requests.length
        wait(30)
        compare(backend.requests.length, idle)
        popup.close()
        list.prefetch(1); list.prefetch(-1); backend.answerPage()
        wait(1)
        compare(backend.requests.length, idle)
        compare(popup.choiceWindow.count, 0)
    }
    function test_mouse_wheel_requests_the_next_page_without_idle_prefetch() {
        popup.show("title", "Title", [], anchor); backend.answerPage()
        const list = findChild(popup.contentItem, "tracksFilterChoices")
        verify(waitForRendering(list))
        list.positionViewAtEnd(); list.forceLayout()
        verify(list.atYEnd)
        compare(backend.requests.length, 1)
        mouseMove(list, list.width / 2, list.height / 2)
        mouseWheel(list, list.width / 2, list.height / 2, 0, -120)
        tryVerify(() => backend.requests.length > 1)
        compare(backend.requests.length, 2)
        backend.answerPage()
        tryCompare(popup.choiceWindow, "count", 200)
        wait(50)
        compare(backend.requests.length, 2, "wheel completion and model changes do not cascade queries")
    }
    function test_table_scrollbars_and_last_row_clear_the_player_data() {
        return [{tag:"normal", space:Metrics.playerHeight}, {tag:"waveform", space:Metrics.playerHeightTall}, {tag:"docked", space:0}]
    }
    function test_table_scrollbars_and_last_row_clear_the_player(data) {
        Metrics.playerSpace = data.space
        const page = createTemporaryObject(screenMaker, test, {width:620, height:500})
        page.columnFilters = ({}); page.typed = ""
        page.shownColumns = ["index", "source", "time", "rating", "quality", "format", "plays", "favorite"]
        const cover = createTemporaryObject(playerCoverMaker, page)
        const rows = page.scrollView, model = rows.model
        for (let i = 0; i < 80; ++i) model.append({trackId:i + 1,rowKind:0,number:i + 1,title:"Track " + i,artist:"Artist",album:"Album",duration:"3:00",plays:0,
            quality:"FLAC",tier:2,available:true,playing:false,selected:false,headerLabel:"",headerMeta:"",art:""})
        const sheet = findChild(page, "tracksSheet")
        const horizontal = findChild(page, "tracksHorizontalScrollbar")
        const vertical = findChild(page, "trackListScrollbar")
        horizontal.viewportHovered = true; vertical.viewportHovered = true
        verify(waitForRendering(page))
        verify(sheet.contentWidth > sheet.width)
        const clearance = data.space > 0 ? data.space + Metrics.spaceWindow : 0
        compare(horizontal.reservedBottom, clearance)
        compare(vertical.reservedBottom, Math.min(rows.bottomMargin, clearance))
        if (data.space > 0) {
            tryVerify(() => horizontal.mapToItem(page, 0, horizontal.height).y <= cover.y)
            tryVerify(() => vertical.mapToItem(page, 0, vertical.height).y <= cover.y)
        }
        const startX = horizontal.contentItem.x + horizontal.contentItem.width / 2
        mouseDrag(horizontal, startX, horizontal.height / 2, horizontal.width - startX, 0)
        tryVerify(() => sheet.atXEnd, 1000, "the table scrollbar remains reachable above the floating player")
        compare(cover.presses, 0)
        sheet.contentX = 0
        const startY = vertical.contentItem.y + vertical.contentItem.height / 2
        mousePress(vertical, vertical.width / 2, startY)
        const grabbed = vertical.pressed
        mouseMove(vertical, vertical.width / 2, vertical.height, 10)
        mouseRelease(vertical, vertical.width / 2, vertical.height)
        verify(grabbed, "the scrollbar receives the press instead of the row pointer surface")
        tryVerify(() => rows.atYEnd, 1000)
        rows.forceLayout()
        tryVerify(() => rows.itemAtIndex(rows.count - 1) !== null)
        const last = rows.itemAtIndex(rows.count - 1)
        verify(last.mapToItem(page, 0, last.height).y <= (data.space > 0 ? cover.y : page.height),
            "last row bottom " + last.mapToItem(page, 0, last.height).y + " exceeds player top " + cover.y + "; bottomMargin=" + rows.bottomMargin + ", contentY=" + rows.contentY)
        verify(last.mapToItem(page, 0, 0).y >= sheet.y)
    }
}
