import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"

ApplicationWindow {
    id: window
    visible: true; width: 1280; height: 800
    color: Theme.color.surfaceWindow
    property string captureFolder: ""
    property int sample: -1
    property int albums: 0
    property int songs: 0
    Component.onCompleted: ThemeModel.mode = "dark"
    AlbumGridModel {
        id: cards
        kind: "source"; key: "app.test.navigation section:albums"
        columns: grid.columns; maxRows: 1
        contentRev: Library.remoteRev + Library.remoteArtRev
    }
    TrackListModel {
        id: tracks
        kind: "source"; key: "app.test.navigation section:tracks"
        filterRev: Library.remoteRev + Library.remoteArtRev
        playingPath: Player.path
    }
    AlbumDetailModel {
        id: detail
        remoteRev: Library.remoteRev + Library.remoteArtRev
    }
    Column {
        id: body
        anchors.fill: parent; anchors.margins: 24; spacing: 16
        AlbumGrid { id: grid; width: parent.width; height: 290; model: cards }
        TrackList { width: parent.width; height: 300; model: tracks }
        SourcePageHeader { id: heading; width: parent.width; cardId: detail.selectedId; record: detail; info: JSON.parse(Sources.card(detail.selectedId, Library.remoteRev) || "{}") }
    }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) { const found = find(child, name); if (found) return found }
        return null
    }
    function fail(message) { console.log("SMOKE FAIL " + message); Qt.exit(1) }
    function audit(item) {
        const match = /^(Fixture (album|track) (\d+))$/.exec(item.title || "")
        if (match && item.hasOwnProperty("aiGenerated") && (item.hasOwnProperty("rowKind") || item.hasOwnProperty("albumId"))) {
            const expected = Number(match[3]) % 3 === 0
            if (item.aiGenerated !== expected) fail("wrong classification for " + match[1])
            if (!!find(item, "aiBadge") !== expected) fail("wrong visible AI badge for " + match[1])
            if (!!find(item, "explicitBadge") !== expected) fail("explicit advisory changed for " + match[1])
            if (match[2] === "album") albums++; else songs++
        }
        for (const child of item.children || []) audit(child)
    }
    Timer {
        interval: 150; running: true; repeat: true
        onTriggered: {
            Sources.refresh()
            if (window.sample < 0) {
                if (cards.albums < 3 || tracks.count < 3) return
                window.sample = 0
                settle.restart()
            }
        }
    }
    Timer {
        id: settle; interval: 1800
        onTriggered: {
            if (window.sample === 0) {
                window.audit(window.contentItem)
                if (window.albums < 3 || window.songs < 3) { window.fail("classified and unclassified rows were not rendered"); return }
            } else {
                const index = window.sample - 1
                if (!detail.found || detail.aiGenerated !== (index === 0)) { window.fail("album detail classification at " + index); return }
                if (!!window.find(heading, "aiBadge") !== (index === 0)) { window.fail("native album header classification at " + index); return }
                const versions = JSON.parse(detail.versions)
                const selected = versions.find(version => version.id === detail.selectedId)
                if (!selected || (index < 2 ? selected.ai_generated !== (index === 0) : selected.ai_generated !== null)) { window.fail("version classification at " + index); return }
            }
            if (++window.sample <= 3) {
                const title = "Fixture album " + (window.sample - 1)
                let index = 0
                while (index < cards.albums && cards.title(index, cards.rev) !== title) index++
                if (index === cards.albums) { window.fail("missing " + title); return }
                detail.albumId = cards.albumId(index, cards.rev)
                settle.restart()
            } else {
                body.grabToImage(result => {
                    result.saveToFile(window.captureFolder + "/source-advisories.png")
                    console.log("SMOKE source advisories completed albums=" + window.albums + " tracks=" + window.songs)
                    Qt.exit(0)
                })
            }
        }
    }
    Timer { interval: 25000; running: true; onTriggered: window.fail("source advisory timeout at " + window.sample) }
}
