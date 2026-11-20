import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/controls/qml/Components" as Controls
import "qrc:/qt/qml/app/chilpa/controls/qml/Patterns" as Chrome
import "qrc:/qt/qml/app/chilpa/query/qml/Components" as Query

TitledFlow {
    id: section
    title: "Collection playback"

    QtObject {
        id: sample
        function queueCollection(action, shuffled, limit, label) {}
    }
    Chrome.CollectionPlayback {
        availableWidth: section.width
        model: sample
        label: "Albums"
        showRandomAlbum: true
        available: true
        filtered: false
        scope: "gallery"
    }
    Controls.CollectionSizeDialog { id: sizes; title: "Shuffle"; shuffled: true; onChosen: close() }
    Query.LibraryCountDialog {
        id: counts
        total: "25 albums · 320 songs"
        counts: ({local: 200, servers: 1, serverTracks: 20, plugins: [["Example source", 100]], showing: "5 albums · 64 songs"})
    }
    PillButton { text: "Queue size"; onClicked: sizes.open() }
    PillButton { text: "Library counts"; onClicked: counts.open() }

    Item {
        width: Metrics.rightPanel
        height: Metrics.queueMin
        Chrome.ContentPlane {
            moving: false
            leftInset: 0
            rightInset: Metrics.spaceWindow
            topInset: 0
            bottomInset: 0
            Rectangle { anchors.fill: parent; color: Theme.color.surfaceSidebar; radius: Metrics.radiusPanel }
        }
    }
    Chrome.RightPanelFrame {
        open: true
        queueShown: true
        height: Metrics.queueMin
    }
}
