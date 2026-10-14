import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/qml"

ApplicationWindow {
    id: window
    width: 1280; height: 800; visible: true
    property var artists: []
    property int sample: 0
    property int step: 0
    property bool awaiting: false
    property double started: 0
    property var mounted: null
    property var failures: []
    property int updates: 0
    readonly property string playbackPath: Player.path
    ScreenHost { id: host; anchors.fill: parent }
    SearchModel { id: search }
    ListView {
        id: found
        width: 1; height: 1500; opacity: 0; z: -1
        model: search
        delegate: Item { required property string title; height: 24 }
    }
    function fail(message) { failures.push(message); console.log("SMOKE FAIL " + message) }
    function descendants(item) {
        let result = [item]
        for (const child of item?.children || []) result = result.concat(descendants(child))
        return result
    }
    function request() {
        mounted = null
        started = Date.now(); awaiting = true
        if (step === 0) {
            search.text = artists[sample][0]
            searchWait.restart()
        } else if (step === 1) Navigation.go("album", artists[sample][2])
        else if (step === 2) Navigation.back()
        else {
            Navigation.openArtist(artists[1 - sample][0])
            Navigation.go("album", artists[sample][2])
            Navigation.back()
        }
    }
    Timer {
        id: searchWait; interval: 30; repeat: true
        onTriggered: {
            for (let row = 0; row < found.count; row++) {
                if (found.itemAtIndex(row)?.title === artists[sample][0]) {
                    stop(); window.started = Date.now(); search.act(row, "open"); return
                }
            }
        }
    }
    function expectedKey() { return "local artist:" + artists[step === 3 ? 1 - sample : sample][0] }
    Connections {
        target: window
        function onFrameSwapped() {
            if (!window.awaiting || !host.item || searchWait.running) return
            const matches = step === 1 ? host.item.albumId === artists[sample][2]
                : host.item.key === window.expectedKey() && host.item.header.title === artists[step === 3 ? 1 - sample : sample][0]
            if (!matches) return
            const elapsed = Date.now() - window.started
            console.log("SMOKE artist sample=" + sample + " step=" + step + " first-frame=" + elapsed + "ms")
            if (elapsed > 500) window.fail("artist navigation took " + elapsed + "ms")
            window.mounted = host.item; window.awaiting = false; settle.restart()
        }
    }
    Timer {
        interval: 50; repeat: true; running: true
        onTriggered: {
            if (!window.mounted || window.awaiting) return
            if (host.item !== window.mounted) window.fail("data remounted the committed page")
            if (window.step !== 1 && host.item.key !== window.expectedKey()) window.fail("the displayed artist disagrees with history")
            window.updates++
        }
    }
    Timer {
        id: settle; interval: 350
        onTriggered: {
            const page = host.item
            if (window.step !== 1) {
                if (page.about.loading === true) { restart(); return }
                const objects = window.descendants(page)
                const grids = objects.filter(item => item.model?.kind === "artist" && item.cardWidth !== undefined)
                if (!grids.length || !grids[0].model.albums) { restart(); return }
                if (window.step === 2 && Math.abs(page.scrollView.contentY - 120) > 2) window.fail("back lost the artist scroll position: " + page.scrollView.contentY)
                const before = page.scrollView.contentHeight
                page.scrollView.contentY = Math.max(0, before - page.scrollView.height)
                page.scrollView.contentY = window.step === 0 ? 120 : 0
                console.log("SMOKE artist data records=" + grids[0].model.albums + " content-height=" + before)
            }
            if (window.step === 3) {
                console.log("SMOKE artist opening completed samples=" + window.artists.length + " frames=" + window.updates + " failures=" + window.failures.length)
                Qt.exit(window.failures.length ? 1 : 0); return
            }
            if (++window.step < 3) { window.request(); return }
            if (++window.sample < window.artists.length) { window.step = 0; window.request(); return }
            window.sample = window.artists.length - 1; window.step = 3; window.request()
        }
    }
    Timer { interval: 1200; running: true; onTriggered: window.request() }
    Timer { interval: 30000; running: true; onTriggered: { window.fail("artist opening timed out at step " + step); Qt.exit(1) } }
}
