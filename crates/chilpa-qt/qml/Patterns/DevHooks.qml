import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"

// The measuring hooks, inert unless an environment variable asks for them
// (see bridge/dev.rs): a frame-time recorder, a screenshot, a start route.
Item {
    id: hooks

    required property Window window

    // A screenshot reads the frame back and encodes it on the window's
    // thread: 120 ms that were counted as one late frame of the scroll.
    // Frames stop being counted once one is taken.
    property bool shot: false

    Connections {
        target: hooks.window
        enabled: Dev.frames && (Dev.scroll === 0 || sweep.running) && !hooks.shot
        function onFrameSwapped() { Dev.frame() }
    }

    // The view with the most content under the window: the one a scroll
    // test is about.
    function longest(item, best) {
        if (item.contentHeight !== undefined && item.interactive && item.contentHeight > item.height
                && (!best || item.contentHeight > best.contentHeight))
            best = item
        for (let i = 0; i < item.children.length; ++i)
            best = longest(item.children[i], best)
        return best
    }

    NumberAnimation {
        id: sweep
        property: "contentY"
        onFinished: Qt.quit()
    }

    Timer {
        running: Dev.scroll > 0
        interval: Metrics.shotDelay
        onTriggered: {
            const view = hooks.longest(hooks.window.contentItem, null)
            if (!view)
                return Qt.quit()
            const distance = Math.min(view.contentHeight - view.height, Dev.scroll * Metrics.scrollTestSeconds)
            sweep.target = view
            sweep.from = 0
            sweep.to = distance
            sweep.duration = distance * 1000 / Dev.scroll
            sweep.start()
        }
    }

    // A grab of the content leaves out the window's own color, so a shot
    // gets a backdrop of its own. Only while shooting: the window's color is
    // the clear color and costs nothing, a full-window rectangle is a fill
    // on every frame.
    Rectangle {
        parent: hooks.window.contentItem
        z: -1
        anchors.fill: parent
        visible: Dev.shot !== ""
        color: Theme.color.surfaceWindow
    }

    Binding {
        target: Dev
        property: "period"
        value: hooks.window.screen ? 1000 / hooks.window.screen.refreshRate : 0
    }

    Timer {
        running: Dev.shot !== ""
        interval: Dev.shotAt
        onTriggered: {
            hooks.shot = true
            hooks.window.contentItem.grabToImage(function(result) {
                result.saveToFile(Dev.shot)
                if (!sweep.running)
                    Qt.quit()
            })
        }
    }

    Component.onCompleted: {
        if (Dev.route === "folders")
            Navigation.openFolder(Dev.routeKey)
        else if (Dev.route === "openMedia" && Dev.routeKey !== "") {
            Navigation.go("openMedia", 0)
            OpenMedia.openPath("sacd", Dev.routeKey)
        }
        else if (Dev.route === "artist")
            Navigation.openArtist(Dev.routeKey)
        else if (Dev.route === "settings")
            Navigation.openSection(Dev.routeKey)
        else if (Dev.route !== "")
            Navigation.go(Dev.route, Dev.routeId)
        if (Dev.addFolder !== "")
            LibraryFolders.add(Dev.addFolder)
    }
}
