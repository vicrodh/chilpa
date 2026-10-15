import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/qml"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"

ApplicationWindow {
    id: window
    width: 1280; height: 800; visible: true
    property var albums: []
    property var titles: []
    property var selectedIds: []
    property int snapshot: 0
    property double previous: 0
    property double worst: 0
    property double worstAfter: 0
    property double previousAfter: 0
    property double mounted: -1
    property double previousAlbum: 0
    property bool fromSearch: false
    property double rapidAlbum: 0
    property int rapidStep: 0
    property bool rapidDone: false
    property bool choiceDone: false
    property var choiceObject: null
    property int choiceRevision: 0
    property double preferenceVersion: 0
    property int sample: 0
    property int phase: 0
    property double started: 0
    property bool awaiting: false
    property real trackHeight: 0
    property var failures: []
    ScreenHost { id: host; anchors.fill: parent }
    SearchModel { id: search; scope: "albums"; source: "local" }
    SearchFlyout { id: results; search: search; room: 700 }
    Component { id: detachedChoice; AlbumDetailModel {} }
    AlbumDetailModel { id: rapidDetail; albumId: window.rapidAlbum }
    TrackListModel { id: rapidTracks; kind: "album"; albumId: rapidDetail.selectedId; albumPage: rapidDetail.tracksRev }
    FrameAnimation {
        running: true
        onTriggered: {
            const now = Date.now()
            if (!window.awaiting) {
                if (window.previousAfter) window.worstAfter = Math.max(window.worstAfter, now - window.previousAfter)
                window.previousAfter = now
                return
            }
            if (window.previous) window.worst = Math.max(window.worst, now - window.previous)
            window.previous = now
        }
    }
    function find(item, name) {
        if (!item) return null
        if (typeof name === "function" ? name(item) : item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    function request() {
        if (sample >= albums.length) {
            if (!rapidDone) { phase = 7; rapid.start(); return }
            if (!choiceDone) {
                choiceObject = detachedChoice.createObject(window, {albumId: albums[1]})
                chooseVersion.start()
                return
            }
            for (const failure of failures) console.log("SMOKE FAIL " + failure)
            console.log("SMOKE album opening completed samples=" + sample + " failures=" + failures.length)
            Qt.exit(failures.length ? 1 : 0)
            return
        }
        phase = 0; fromSearch = sample === 1 || sample === 5
        if (fromSearch) {
            search.text = titles[sample]
            results.open()
            search.source = "local"
            poll.start()
            return
        }
        begin()
        Navigation.go("album", albums[sample])
    }
    function begin() {
        previousAlbum = Navigation.routeId
        awaiting = true; started = Date.now(); previous = started; worst = 0; mounted = -1
    }
    function contentBuilds(view) {
        let total = 0
        for (let i = 0; i < view.count; i++) total += view.itemAtIndex(i)?.contentBuilds || 0
        return total
    }
    function check() {
        const page = host.item, view = find(page, "trackListView")
        if (phase === 0) {
            const builds = contentBuilds(view)
            console.log("SMOKE album after-first-frame=" + sample + " worst-gui-gap=" + worstAfter + "ms content-builds=" + builds)
            if (worstAfter > 250) failures.push("record blocked the GUI for " + worstAfter + "ms after its first frame")
            if (view.model.count > 100 && builds >= 60) failures.push("record constructed " + builds + " visual contents before scrolling")
            trackHeight = view.contentHeight
            snapshot = view.model.albumPage
            Library.remoteRev++
            Library.remoteArtRev++
            phase = 1; next.restart(); return
        }
        if (phase === 1) {
            if (view.model.albumPage !== snapshot) failures.push("remote artwork reloaded a local record")
            worstAfter = 0; previousAfter = Date.now()
            page.contentY = Math.max(0, page.contentHeight - page.height)
            phase = 2; next.restart(); return
        }
        if (phase === 2) {
            console.log("SMOKE album scroll-bottom=" + sample + " worst-gui-gap=" + worstAfter + "ms")
            if (worstAfter > 250) failures.push("scrolling down blocked the GUI for " + worstAfter + "ms")
            const last = view.itemAtIndex(view.count - 1)
            if (!last || !find(last, "trackRowBody")) failures.push("last track has no visual row")
            if (view.contentHeight !== trackHeight) failures.push("windowing changed track geometry")
            worstAfter = 0; previousAfter = Date.now()
            page.contentY = 0
            phase = 3; next.restart(); return
        }
        if (phase === 3) {
            console.log("SMOKE album scroll-top=" + sample + " worst-gui-gap=" + worstAfter + "ms")
            if (worstAfter > 250) failures.push("scrolling up blocked the GUI for " + worstAfter + "ms")
        }
        if (phase === 3 && sample === 0) {
            page.showInformation()
            phase = 4; next.restart(); return
        }
        if (phase === 4) {
            const popup = find(page, "albumInformationLoader")?.item
            if (!popup || !popup.opened) failures.push("record information failed to open")
            if (popup) popup.close()
            page.showActions(page, 30, 30)
            phase = 5; next.restart(); return
        }
        if (phase === 5) {
            const popup = find(page, "albumActionMenuLoader")?.item
            if (!popup || !popup.opened) failures.push("record menu failed to open")
            if (popup) popup.close()
        }
        sample++; request()
    }
    Connections {
        target: host
        function onItemChanged() {
            if (window.awaiting && host.item && host.item.albumId === Navigation.routeId)
                window.mounted = Date.now() - window.started
        }
    }
    Connections {
        target: window
        function onFrameSwapped() {
            if (!window.awaiting || !host.item || host.item.albumId !== Navigation.routeId) return
            if (window.fromSearch ? Navigation.routeId === window.previousAlbum : host.item.albumId !== window.albums[window.sample]) return
            if (window.mounted < 0) window.mounted = Date.now() - window.started
            const elapsed = Date.now() - window.started
            const view = window.find(host.item, "trackListView")
            if (!view || !view.model.ready || !view.model.count) return
            let rows = 0
            for (let i = 0; i < view.count; i++) {
                const row = view.itemAtIndex(i)
                if (row && window.find(row, "trackRowBody")) rows++
            }
            if (!rows) return
            const builds = window.contentBuilds(view)
            window.selectedIds[window.sample] = view.model.albumId
            console.log("SMOKE album=" + window.albums[window.sample] + " selected=" + view.model.albumId + " search-click=" + window.fromSearch + " mount=" + window.mounted + "ms first-frame-with-tracks=" + elapsed + "ms worst-gui-gap=" + window.worst + "ms tracks=" + view.model.count + " visual-rows=" + rows + " content-builds=" + builds)
            if (elapsed > 500) window.failures.push("local record took " + elapsed + "ms to present its tracks")
            if (view.model.count > 100 && (rows >= 60 || builds >= 60)) window.failures.push("record built " + builds + " visual contents")
            window.awaiting = false
            window.previousAfter = Date.now(); window.worstAfter = 0
            next.restart()
        }
    }
    Timer {
        id: poll; interval: 30; repeat: true
        onTriggered: {
            const hit = window.find(results.contentItem, item => item.kind === 3 && item.title === window.titles[window.sample])
            if (!hit) return
            stop()
            window.begin()
            results.choose(hit.index)
            console.log("SMOKE album search dispatch=" + (Date.now() - window.started) + "ms")
        }
    }
    Timer {
        id: rapid; interval: 15; repeat: true
        onTriggered: {
            const sequence = [window.albums[0], window.albums[1], window.albums[2], window.albums[1], window.albums[0]]
            window.rapidAlbum = sequence[window.rapidStep++]
            if (window.rapidStep < sequence.length) return
            stop(); settled.start()
        }
    }
    Timer {
        id: settled; interval: 800
        onTriggered: {
            if (!rapidDetail.found || !rapidTracks.ready || rapidDetail.selectedId !== window.selectedIds[0] || rapidTracks.albumId !== rapidDetail.selectedId)
                window.failures.push("rapid album navigation published an obsolete response")
            console.log("SMOKE rapid album navigation selected=" + rapidDetail.selectedId + " tracks=" + rapidTracks.count)
            window.rapidDone = true; window.request()
        }
    }
    Timer {
        id: chooseVersion; interval: 30; repeat: true
        onTriggered: {
            const detail = window.choiceObject
            if (!detail || !detail.found) return
            stop()
            const versions = JSON.parse(detail.versions || "[]")
            const other = versions.find(version => version.id !== detail.selectedId)
            if (!other) {
                console.log("SMOKE album preference skipped: the record has one edition")
                detail.destroy(); window.choiceObject = null
                window.choiceDone = true; window.request(); return
            }
            window.preferenceVersion = other.id
            window.choiceRevision = Library.contentRev
            detail.selectVersion(String(other.id))
            detail.contentRev++
            detail.destroy(); window.choiceObject = null
            Library.contentRev++
            choiceSettled.start()
        }
    }
    Timer {
        id: choiceSettled; interval: 800
        onTriggered: {
            if (Library.contentRev <= window.choiceRevision + 1) window.failures.push("destroying the album page lost the version preference notification")
            console.log("SMOKE album preference persisted album=" + window.albums[1] + " version=" + window.preferenceVersion)
            window.choiceDone = true; window.request()
        }
    }
    Timer { id: next; interval: 600; onTriggered: window.check() }
    Timer { interval: 1000; running: true; onTriggered: window.request() }
    Timer { interval: 25000; running: true; onTriggered: { console.log("SMOKE FAIL album opening timed out"); Qt.exit(1) } }
}
