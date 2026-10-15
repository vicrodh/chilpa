import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/qml"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

ApplicationWindow {
    id: window
    width: 1280; height: 800; visible: true
    color: Theme.color.surfaceWindow
    Component.onCompleted: ThemeModel.mode = "dark"
    property var destinations: [
        "app.test.navigation native:discover:home", "app.test.navigation section:library",
        "app.test.navigation section:tracks", "app.test.navigation section:albums",
        "app.test.navigation section:artists", "app.test.navigation section:labels",
        "app.test.navigation section:playlists", "app.test.navigation section:library",
        "app.test.second section:tracks", "app.test.second section:artists", "app.test.second section:library",
        "app.test.second native:discover:home", "app.test.navigation native:discover:home"
    ]
    property int sample: 0
    property double started: 0
    property double presented: 0
    property var mounted: null
    property bool awaiting: false
    property var failures: []
    property int audited: 0
    property int presentedFrames: 0
    property string auditOrigin: ""
    property real expectedScroll: 0
    property var heights: ({})
    property var moduleGeometry: ({})
    property var moduleSlots: ({})
    property int modules: 0
    property int skeletons: 0
    property string captureFolder: ""
    ScreenHost { id: host; anchors.fill: parent }
    Timer { interval: 150; running: true; repeat: true; onTriggered: Sources.refresh() }
    function fail(message) {
        const key = sample + ": " + message
        if (failures.indexOf(key) < 0) { failures.push(key); console.log("SMOKE FAIL " + key) }
    }
    function request() {
        if (sample >= destinations.length) {
            console.log("SMOKE sources completed samples=" + sample + " frames=" + audited + " modules=" + modules + " skeletons=" + skeletons + " failures=" + failures.length)
            Qt.exit(failures.length ? 1 : 0)
            return
        }
        started = Date.now(); awaiting = true; mounted = null; heights = ({}); moduleGeometry = ({}); moduleSlots = ({})
        expectedScroll = sample === 7 ? 120 : 0
        Navigation.goKey("source", destinations[sample])
    }
    function auditChildren(item) {
        if (item.objectName === "homeModuleLoader" && item.parent) {
            const slot = item.parent
            const key = String(slot.entry.key)
            if (destinations[sample].endsWith("section:library")) {
                const previous = heights[key]
                const geometry = {
                    audit: auditOrigin, frame: presentedFrames,
                    elapsed: Date.now() - started, height: slot.height, width: slot.width,
                    visible: slot.visible, ready: slot.ready, shown: slot.shown,
                    y: slot.mapToItem(host.item, 0, 0).y,
                    parentVisible: slot.parent.visible, parentHeight: slot.parent.height,
                    loaderStatus: item.status, loaderHeight: item.height, loaderVisible: item.visible,
                    itemReady: item.item?.ready, itemShown: item.item?.shown,
                    itemVisible: item.item?.visible, modelKey: item.item?.modelKey,
                    loadState: item.item?.loadState, section: item.item?.section,
                    sameDelegate: moduleSlots[key] === slot,
                    leaving: host.item.leaving, pageVisible: host.item.visible,
                    viewportHeight: host.item.height, contentY: host.item.contentY
                }
                if (previous !== undefined && slot.height < previous - 2) {
                    console.log("SMOKE contraction previous " + key + " " + JSON.stringify(moduleGeometry[key]))
                    console.log("SMOKE contraction current " + key + " " + JSON.stringify(geometry))
                    fail("module contracted " + key + " from " + previous + " to " + slot.height)
                }
                heights[key] = slot.height
                moduleGeometry[key] = geometry
                moduleSlots[key] = slot
            }
            if (!slot.ready) skeletons++
            else modules++
        }
        if (item.hasOwnProperty("cardWidth") && item.hasOwnProperty("rowHeight") && item.columns > 0 && !item.list) {
            if (item.cardWidth !== 200) fail("card width changed to " + item.cardWidth)
        }
        for (const child of item.children || []) auditChildren(child)
    }
    function audit(origin) {
        auditOrigin = origin
        if (awaiting || !mounted) return
        if (host.item !== mounted) fail("the committed page was remounted after data arrived")
        const viewport = host.item.contentY !== undefined ? host.item : host.item.scrollView
        if (viewport) {
            const offset = viewport.contentY - ((viewport.originY || 0) - (viewport.topMargin || 0))
            if (Math.abs(offset - expectedScroll) > 2) {
                fail("scroll changed to " + offset + " instead of " + expectedScroll)
                if (!heights["diagnosed"]) {
                    console.log("SMOKE geometry contentY=" + viewport.contentY + " originY=" + viewport.originY + " headerY=" + viewport.headerItem?.y + " headerHeight=" + viewport.headerItem?.height + " headerAt=" + viewport.headerItem?.mapToItem(viewport, 0, 0).y)
                    heights["diagnosed"] = true
                }
            }
            if (expectedScroll === 0 && host.item.headerItem && host.item.headerItem.mapToItem(host.item, 0, 0).y < 0) fail("header left the viewport")
        }
        audited++
        auditChildren(host.item)
    }
    Connections {
        target: window
        function onFrameSwapped() {
            presentedFrames++
            if (awaiting && host.committedRoute.split("\u001f")[3] === destinations[sample]) {
                const elapsed = Date.now() - started
                console.log("SMOKE source " + sample + " first-frame=" + elapsed + "ms")
                if (elapsed > 500) fail("navigation took " + elapsed + "ms")
                mounted = host.item; awaiting = false; presented = Date.now()
                host.grabToImage(result => result.saveToFile(captureFolder + "/source-" + sample + "-first.png"))
                settle.restart()
            }
            audit("frameSwapped")
        }
    }
    Timer { interval: 16; running: true; repeat: true; onTriggered: window.audit("timer") }
    Timer {
        id: settle; interval: 1800
        onTriggered: {
            window.audit("settle")
            const index = window.sample
            host.grabToImage(result => result.saveToFile(window.captureFolder + "/source-" + index + "-loaded.png"))
            if (window.sample === 1) {
                const view = host.item
                view.parent.scroll.cancel()
                view.contentY = (view.originY || 0) - (view.topMargin || 0) + 120
            }
            window.sample++; window.request()
        }
    }
    Timer { interval: 1400; running: true; onTriggered: window.request() }
    Timer { interval: 35000; running: true; onTriggered: { window.fail("timeout"); Qt.exit(1) } }
}
