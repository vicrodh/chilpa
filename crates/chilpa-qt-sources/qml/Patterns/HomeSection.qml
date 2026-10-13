import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// I reserve a module's footprint before building it. The vertical view
// instantiates only its viewport and runway; offscreen modules do no work.
Item {
    id: slot
    property var entry: ({})
    property Component content
    property bool started: false
    clip: true
    readonly property bool ready: body.status === Loader.Ready && body.item && body.item.ready === true
    readonly property bool shown: !ready || body.item.shown
    readonly property bool trackRows: entry.kind === "tracks" || entry.tracks === true || !!entry.tracks_key || entry.presentation === "ranked"
    readonly property var footprint: Visit.recall("home.module." + entry.id, null)
    height: ready ? (shown ? body.height : 0)
        : footprint && Math.abs(footprint.width - width) < Metrics.gap2 ? footprint.height : placeholder.height
    function remember() {
        if (ready) Visit.keep("home.module." + entry.id, { width: width, height: shown ? body.height : 0 })
    }
    onReadyChanged: Qt.callLater(remember)
    onHeightChanged: if (ready) Qt.callLater(remember)

    Column {
        id: placeholder
        width: parent.width
        visible: !slot.ready
        spacing: slot.entry.kind === "shelf" ? Metrics.gap12 : Metrics.gap8
        SectionHeader { width: parent.width; title: slot.entry.title || ""; sourceIcon: slot.entry.sourceIcon || "" }
        Item {
            visible: !!slot.entry.header
            width: parent.width
            height: visible ? Math.min(Metrics.heroArt, width / 3) : 0 // derived: the source header's portrait size
            Rectangle { width: parent.height; height: width; radius: Metrics.radiusCard; color: Theme.color.stateHover }
            Column {
                x: parent.height + Metrics.heroGap
                width: parent.width - x
                spacing: Metrics.gap12
                Rectangle { width: parent.width * 0.7; height: Metrics.gap24; radius: Metrics.gap4; color: Theme.color.stateHover }
                Rectangle { width: parent.width * 0.5; height: Metrics.gap12; radius: Metrics.gap4; color: Theme.color.stateHover }
            }
        }
        Rectangle {
            visible: slot.entry.sortable === true || slot.entry.tracks === true || !!slot.entry.tracks_key
            width: Metrics.sceneSearchWidth; height: Metrics.button32
            radius: Metrics.radiusControl; color: Theme.color.stateHover
        }
        ShelfSkeleton {
            width: parent.width
            tracks: slot.trackRows
            round: slot.entry.round === true
            oneRow: slot.entry.kind !== "albums" || !slot.entry.size
            count: slot.entry.size || (tracks ? slot.entry.kind === "tracks" ? Metrics.mostPlayedLimit
                : slot.entry.presentation === "ranked" ? Metrics.shelfItems : Metrics.shelfTrackLimit : Metrics.shelfLimit)
            trackRowHeight: slot.entry.presentation === "ranked" || SettingsModel.density === "rich" ? Metrics.rowRich : Metrics.rowCompact
            fixedCardWidth: slot.entry.kind === "playlists" ? Metrics.cardMinWidth : 0
        }
        Item { width: parent.width; height: slot.entry.kind === "shelf" && !slot.trackRows ? Metrics.gap8 : 0 }
    }
    Loader {
        id: body
        objectName: "homeModuleLoader"
        width: parent.width
        height: item ? item.implicitHeight : 0
        active: slot.started
        asynchronous: true
        visible: slot.ready && slot.shown
        sourceComponent: slot.content
        onLoaded: item.entry = Qt.binding(() => slot.entry)
    }
}
