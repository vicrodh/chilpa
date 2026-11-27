import QtQuick
import QtTest
import app.chilpa
import app.chilpa.controls
import app.chilpa.pages
import app.chilpa.sources
import "../../qml/Screens"
import "qrc:/qt/qml/app/chilpa/controls/qml/Patterns" as Driving
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns" as Kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as Primitives
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme" as KitTheme

TestCase {
    id: test
    name: "SourceTrackRecycling"
    when: windowShown
    visible: true
    width: 1000; height: 600

    readonly property string tracksKey: "fixture tracks"
    readonly property string otherKey: "fixture other tracks"
    Component { id: listing; SourceListing { width: test.width; height: test.height; source: test.tracksKey } }
    Component { id: keys; Driving.WindowControls { window: test.Window.window; outputDetailsHost: false } }
    Component {
        id: routedListing
        Kit.PageTransition {
            id: router
            anchors.fill: parent
            sourceComponent: Component {
                NativeSourceHost {
                    source: router.routeKeyOf(parent)
                    presentation: JSON.parse(NativePages.presentation(source, Library.remoteRev + Sources.revision))
                }
            }
        }
    }
    function document(extra) {
        return JSON.stringify(Object.assign({music:true, albumsOnly:false, entity:"track", title:"Saved tracks"}, extra))
    }
    function init() {
        Sources.pages = {[tracksKey]:document({}), [otherKey]:document({title:"Other tracks"})}
        SettingsModel.preferences = ({})
        Primitives.Visit.kept = ({})
    }
    function cleanup() { Hotkeys.answer = ""; KitTheme.Motion.reduced = false }

    function row(i, grouped) {
        const header = grouped && i % 20 === 0
        return {trackId:header ? 0 : i + 1, rowKind:header ? 1 : 0, number:i + 1, title:header ? "" : "Track " + i,
            artist:"Band", album:"Record", sourceName:"", sourceInfo:"{}", composer:"", duration:"3:00", plays:0,
            quality:"FLAC", tier:2, available:true, playing:false, selected:false,
            headerLabel:header ? "Group " + i : "", headerMeta:"", art:""}
    }
    // Like tst_track_window: attach the populated source model once, not thousands of animated inserts.
    function populate(view, count, grouped) {
        const list = view.parent, tracks = list.model
        list.model = null
        tracks.clear()
        for (let i = 0; i < count; ++i) tracks.append(row(i, grouped))
        list.model = tracks
        view.forceLayout()
        tryCompare(view, "count", count)
        tryVerify(() => view.height > 0 && slotCount(view) > 0)
    }
    function slotCount(view) {
        let n = 0
        for (let i = 0; i < view.count; ++i) if (view.itemAtIndex(i)) ++n
        return n
    }
    function bounded(view) {
        const n = slotCount(view)
        verify(n > 0 && n < 100, "Expected viewport-sized allocation, got " + n + " assigned slots for " + view.count + " rows")
    }
    function bodyCount(view) {
        let n = 0
        for (let i = 0; i < view.count; ++i) {
            const slot = view.itemAtIndex(i)
            if (slot && findChild(slot, "trackRowBody")) ++n
        }
        return n
    }
    function centerInside(item, viewport) {
        if (!item || !item.visible || item.width <= 0 || item.height <= 0) return false
        const p = item.mapToItem(viewport, item.width / 2, item.height / 2)
        return p.x >= 0 && p.x < viewport.width && p.y >= 0 && p.y < viewport.height
    }
    function shows(view, viewport, at, id, title) {
        const slot = view.itemAtIndex(at)
        const label = slot && findChild(slot, "creditedText")
        return slot && slot.trackId === id && label && label.text === title && centerInside(label, viewport)
            && centerInside(label, test)
    }
    function showsTrack(view, viewport, at) {
        tryVerify(() => shows(view, viewport, at, at + 1, "Track " + at), 5000, "Visible title and ID for track at " + at)
    }
    function test_thousands_of_tracks_recycle_slots_data() {
        return [{tag:"plain", grouped:false}, {tag:"grouped", grouped:true}]
    }
    function test_thousands_of_tracks_recycle_slots(data) {
        const page = createTemporaryObject(listing, test)
        const view = findChild(page, "trackListView")
        populate(view, 4000, data.grouped)
        // This must fail on the original fit-content listing for retaining slots, not for lacking a new property.
        waitForPolish(view)
        view.forceLayout()
        bounded(view)
        compare(view.itemAtIndex(3999), null)
        compare(page.scrollView, view)
        compare(view.parent.fitContent, false)
        compare(view.parent.viewport, null)
        verify(view.interactive && view.reuseItems && view.height < page.height)
        verify(!page.interactive)
        const order = findChild(page, "sourceOrder"), controlsY = order.mapToItem(page, 0, 0).y
        for (const at of [1001, 3001, 3999, 2001, 1]) { // never a group header
            view.positionViewAtIndex(at, ListView.Center)
            showsTrack(view, view, at)
            bounded(view)
            const before = view.contentY, delta = at === 3999 ? -12 : 12
            for (let step = 0; step < 8; ++step) {
                view.contentY += delta
                wait(1)
                view.forceLayout()
                bounded(view)
            }
            verify((view.contentY - before) * delta > 0, "Incremental scroll moved in the requested direction")
            let middle = view.indexAt(1, view.contentY + view.height / 2)
            verify(middle >= 0, "The middle of the viewport must not go blank")
            if (data.grouped && middle % 20 === 0) ++middle
            showsTrack(view, view, middle)
            compare(order.mapToItem(page, 0, 0).y, controlsY)
            verify(centerInside(order, page))
        }
        compare(view.itemAtIndex(3999), null)
    }

    function test_whole_page_fallback_data() {
        return [{tag:"folder-and-tracks"}, {tag:"album-and-tracks"}, {tag:"short-window"}]
    }
    // Capture live items only on assertion failure, before QtTest destroys the page.
    function checkedWithEvidence(check, evidence) {
        try { check() } catch (error) {
            console.info("SourceTrackRecycling evidence:", JSON.stringify(evidence()))
            throw error
        }
    }
    function box(item, relativeTo) {
        if (!item) return null
        const p = item.mapToItem(relativeTo, 0, 0)
        return {x:p.x, y:p.y, w:item.width, h:item.height, visible:item.visible}
    }
    function scrollState(v) {
        return {y:v.contentY, origin:v.originY, height:v.height, content:v.contentHeight, interactive:v.interactive}
    }
    function rowEvidence(page, view, outer, at) {
        const list = view.parent, model = list.model, owner = page.scrollView || outer
        const slot = view.itemAtIndex(at), role = model && at < model.count ? model.get(at) : null
        const body = slot && findChild(slot, "trackRowBody"), label = slot && findChild(slot, "creditedText")
        // Independent mapping into delegate coordinates, including the inner contentItem offset.
        const top = owner.mapToItem(view.contentItem, 0, 0).y
        const loader = slot && Array.from(slot.children).find(child => child.sourceComponent === slot.content)
        return {at, count:view.count, owner:page.scrollView?.objectName ?? null, mappedIn:owner.objectName,
            listViewport:list.viewport?.objectName ?? null,
            fitContent:list.fitContent, inner:scrollState(view), outer:scrollState(outer), listY:list.y,
            visibleTop:list.visibleTop, mappedTop:top, middle:view.indexAt(1, top + owner.height / 2),
            role:role && [role.rowKind, role.trackId, role.title],
            slot:slot && {index:slot.index, kind:slot.rowKind, id:slot.trackId, title:slot.title, y:slot.y,
                box:box(slot, owner), layoutReady:slot.layoutReady, immediate:slot.immediate,
                inViewport:slot.inViewport, loaderStatus:loader?.status, builds:slot.contentBuilds}, body:box(body, owner),
            label:label && {text:label.text, owner:box(label, owner), test:box(label, test)}, testSize:[test.width,test.height]}
    }
    function findVisible(item, predicate) {
        if (!item.visible) return null
        if (predicate(item)) return item
        for (const child of item.children) {
            const found = findVisible(child, predicate)
            if (found) return found
        }
        return null
    }
    function test_whole_page_fallback(data) {
        const folder = data.tag === "folder-and-tracks", album = data.tag === "album-and-tracks"
        if (folder) Sources.pages = {[tracksKey]:document({folders:[{title:"Child folder", key:"fixture child"}]})}
        const page = createTemporaryObject(listing, test, {height:folder || album ? 600 : 100})
        const grid = findChild(page, "sourceListingAlbums")
        if (album) {
            grid.model.append({first:0, span:1, group:"", revision:0})
            grid.model.albums = 1
            tryVerify(() => grid.visible && grid.scrollView.itemAtIndex(0) !== null)
        }
        const view = findChild(page, "trackListView"), outer = page
        populate(view, 247, false)
        tryCompare(view.parent, "fitContent", true)
        compare(page.scrollView, outer)
        compare(view.parent.viewport, outer)
        verify(outer.interactive && !view.interactive)
        const control = folder ? findVisible(page, item => item.text === "Child folder")
            : album ? findVisible(grid, item => item.title === "Record 0" && item.albumId === -1)
            : findChild(page, "sourceListingSearch")
        verify(control, "The mixed content or short-window controls must actually be reachable")
        // populate() lays out the inner ListView, not the album grid and enclosing Column.
        checkedWithEvidence(() => verify(waitForPolish(test.Window.window), "Mixed-page layout before scrolling"),
            () => rowEvidence(page, view, outer, 246))
        outer.contentY = Math.max(0, control.mapToItem(outer.contentItem, 0, control.height / 2).y - outer.height / 2)
        tryVerify(() => centerInside(control, outer))
        tryVerify(() => view.itemAtIndex(246) !== null)
        compare(findChild(view.itemAtIndex(246), "trackRowBody"), null)
        const last = view.itemAtIndex(246)
        const target = last.mapToItem(outer.contentItem, 0, last.height / 2).y - outer.height / 2
        const end = outer.originY + outer.contentHeight - outer.height
        outer.contentY = target
        outer.returnToBounds()
        checkedWithEvidence(() => {
            tryVerify(() => !outer.moving && outer.contentY >= outer.originY
                && outer.contentY <= outer.originY + outer.contentHeight - outer.height + 1)
            showsTrack(view, outer, 246)
        }, () => ({row:rowEvidence(page, view, outer, 246), target, end,
            grid:box(grid, outer.contentItem), control:box(control, outer), albums:grid.model.albums}))
        verify(!centerInside(control, outer), "The controls and rows scroll together")
        verify(outer.contentY > outer.height)
        tryVerify(() => !findChild(view.itemAtIndex(0), "trackRowBody"))
        const built = bodyCount(view)
        verify(built > 0 && built < 100, "Mixed pages keep the slots, not all " + view.count + " row bodies")
        // Removing the constraint must give scrolling back to the bounded list.
        if (folder) Sources.pages = {[tracksKey]:document({})}
        else if (album) { grid.model.clear(); grid.model.albums = 0 }
        else page.height = 600
        tryCompare(page, "scrollView", view)
        tryCompare(outer, "contentY", 0)
        tryVerify(() => { const n = slotCount(view); return n > 0 && n < 100 })
    }

    function test_controls_and_focused_row_menu() {
        const page = createTemporaryObject(listing, test)
        const view = findChild(page, "trackListView"), list = view.parent, tracks = list.model
        const order = findChild(page, "sourceOrder"), search = findChild(page, "sourceListingSearch")
        order.picked("title-desc")
        order.parent.grouping = "artist"
        search.text = "Band"
        compare(tracks.key.split("\u001f"), [tracksKey, "title:desc", "artist", "Band"])
        search.focusInput(); keyClick(Qt.Key_Escape)
        compare(tracks.key.split("\u001f")[3], "")
        populate(view, 300, true)
        const controls = createTemporaryObject(keys, test)
        verify(controls && controls.listening)
        const outer = page
        let stage = "layout", menu = null, beforeLast = null, afterLast = null
        Hotkeys.keyEvents = []
        checkedWithEvidence(() => {
            verify(waitForPolish(test.Window.window), "Listing layout before navigation")
            compare(page.scrollView, view)
            compare(list.viewport, null)
            stage = "first non-header"
            list.viAction("first"); compare(list.cursorRow, 1)
            stage = "Shift+G / list.last"
            beforeLast = scrollState(view)
            sendListKey("list.last", Qt.Key_G, Qt.ShiftModifier)
            afterLast = scrollState(view)
            compare(list.cursorRow, 299)
            showsTrack(view, view, 299)
            verify(list.activeFocus)
            compare(controls.focusedList(), list)
            compare(tracks.lastAction, "select")
            compare(tracks.lastRow, 299)
            stage = "O / list.menu"
            sendListKey("list.menu", Qt.Key_O, Qt.NoModifier)
            menu = Array.from(list.data).find(child => child.model === tracks && child.entryItems !== undefined && child.row !== undefined)
            verify(menu)
            tryCompare(menu, "opened", true)
            compare(menu.row, 299)
            const queueEntry = () => Array.from(menu.entryItems).find(item => item.entry?.act === "queue")
            tryVerify(() => !!queueEntry())
            stage = "click queue"
            tracks.lastAction = ""; tracks.lastRow = -1
            mouseClick(queueEntry(), queueEntry().width / 2, queueEntry().height / 2)
            tryCompare(tracks, "lastAction", "queue")
            compare(tracks.lastRow, 299)
            tryCompare(menu, "visible", false)
            compare(findChild(page, "trackListView"), view)
        }, () => ({stage, beforeLast, afterLast, row:rowEvidence(page, view, outer, 299), cursor:list.cursorRow,
            focus:String(controls.window.activeFocusItem), focusedList:controls.focusedList() === list,
            blocked:Primitives.InputLayers.blocksShortcuts(controls.window), searchActive:Hotkeys.searchActive,
            listening:controls.listening, scope:Hotkeys.scopeAsked, events:Hotkeys.keyEvents,
            action:[tracks.lastAction, tracks.lastRow], menu:menu && [menu.row, menu.opened]}))
    }
    function sendListKey(action, key, modifiers) {
        // Only binding lookup is mocked: assert that the physical key reached Hotkeys.pressedIn.
        const start = Hotkeys.keyEvents.length
        Hotkeys.answer = action
        try { keyClick(key, modifiers) } finally { Hotkeys.answer = "" }
        compare(Hotkeys.scopeAsked, "window")
        verify(Hotkeys.keyEvents.slice(start).some(e => e.key === key && e.modifiers === modifiers && !e.typing),
            "WindowControls received the focused list key for " + action)
    }

    function placeInside(view, at) {
        view.positionViewAtIndex(at, ListView.Beginning)
        view.contentY += 7
        showsTrack(view, view, at)
        compare(view.indexAt(1, view.contentY), at)
        compare(view.contentY - view.itemAtIndex(at).y, 7)
    }
    function test_same_key_model_reset_restores_row_and_inset() {
        const page = createTemporaryObject(listing, test)
        const view = findChild(page, "trackListView"), tracks = view.parent.model
        populate(view, 350, false)
        const at = 220, key = tracks.key
        placeInside(view, at)
        // QML ListModel edits emit row signals, not the bridge's reset pair. Deliver both notifications
        // to the real connections, with the same key, changed data and a visibly rewound viewport.
        tracks.modelAboutToBeReset()
        tracks.setProperty(at, "trackId", 9001); tracks.setProperty(at, "title", "Refreshed song")
        view.positionViewAtBeginning()
        compare(view.indexAt(1, view.contentY), 0)
        tracks.modelReset()
        tryVerify(() => shows(view, view, at, 9001, "Refreshed song") && view.contentY - view.itemAtIndex(at).y === 7)
        compare(tracks.key, key)
    }

    function route(key) { return [0, "source", 0, key].join("\u001f") }
    function mounted(router, key) {
        tryCompare(router, "committedRoute", route(key))
        tryCompare(router, "awaitingContentFrame", false)
        compare(router.item.source, key)
        const view = findChild(router.item.item, "trackListView")
        verify(view, "NativeSourceHost mounted the source listing")
        return view
    }
    function test_return_after_delayed_rows_restores_inner_view_data() {
        return [{tag:"within-loading-page", at:3}, {tag:"beyond-loading-page", at:220}]
    }
    function test_return_after_delayed_rows_restores_inner_view(data) {
        KitTheme.Motion.reduced = true
        const router = createTemporaryObject(routedListing, test, {route:route(tracksKey)})
        const first = mounted(router, tracksKey)
        populate(first, 350, false)
        tryCompare(router.item, "scrollView", first)
        wait(0) // let the initial owner change restore its initial position before user scrolling
        placeInside(first, data.at)
        const saved = first.contentY - first.originY
        verify(saved > 0)
        router.route = route(otherKey)
        const other = mounted(router, otherKey)
        populate(other, 100, false)
        tryCompare(router.item, "scrollView", other)
        compare(other.contentY - other.originY, 0)
        compare(Primitives.Visit.recall(route(tracksKey), -1), saved)
        // Do not prime the returned list or call restore. The new page commits and paints empty;
        // only afterwards may the source publish rows and change the viewport the router observes.
        Sources.pages = {[tracksKey]:document({loading:true}), [otherKey]:document({title:"Other tracks"})}
        router.route = route(tracksKey)
        const back = mounted(router, tracksKey)
        compare(back.count, 0)
        compare(router.item.item.about.loading, true)
        wait(0)
        const tracks = back.parent.model
        for (let i = 0; i < 350; ++i) tracks.append(row(i, false))
        Sources.pages = {[tracksKey]:document({loading:false}), [otherKey]:document({title:"Other tracks"})}
        back.forceLayout()
        tryCompare(back, "count", 350)
        tryCompare(router.item, "scrollView", back)
        tryVerify(() => Math.abs(back.contentY - back.originY - saved) < 1, 5000,
            "The initially empty page must not consume or overwrite the saved inner offset " + saved)
        showsTrack(back, back, data.at)
        compare(back.contentY - back.itemAtIndex(data.at).y, 7)
    }
}
