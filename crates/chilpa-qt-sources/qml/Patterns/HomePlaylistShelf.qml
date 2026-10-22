import "qrc:/qt/qml/app/chilpa/playlists/qml/Components"
import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// I show recently listened playlists from the local library or every source.
Column {
    id: shelf
    property var entry: ({})
    readonly property bool ready: true
    readonly property var lists: JSON.parse(Playlists.recent(entry.allSources === true, entry.size || Metrics.shelfLimit, Playlists.rev + Library.listeningRev + Library.remoteRev) || "[]")
    readonly property var panels: JSON.parse(Sources.snapshot || "{}").panels || []
    readonly property bool shown: lists.length > 0
    readonly property var shownLists: lists.slice(0, entry.size || CardLayout.columns(width))
    readonly property real farthest: Math.max(0, rail.contentWidth - rail.width)
    readonly property real step: CardLayout.columns(width) * (CardLayout.cardWidth + Metrics.gridGap) // derived: one viewport of playlist cards
    spacing: Metrics.gap8
    function openPlaylist(id) {
        const window = shelf.Window.window
        if (typeof window?.deferNavigation === "function") window.deferNavigation(Playlists.open, id)
        else Playlists.open(id)
    }
    SectionHeader {
        width: parent.width; title: shelf.entry.title || ""
        paged: shelf.farthest > 0
        canBack: rail.contentX > 1; canForward: rail.contentX < shelf.farthest - 1
        onBack: rail.contentX = Math.max(0, rail.contentX - shelf.step)
        onForward: rail.contentX = Math.min(shelf.farthest, rail.contentX + shelf.step)
    }
    Flickable {
        id: rail
        objectName: "homePlaylistRail"
        width: parent.width; height: cards.implicitHeight + Metrics.gap8
        contentWidth: cards.width; contentHeight: height; clip: true
        interactive: contentWidth > width; flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.horizontal: ViewportScrollBar {}
        Row {
            id: cards; spacing: Metrics.gridGap
            Repeater {
                model: shelf.shownLists
                PlaylistCard {
                    required property var modelData
                    readonly property var panel: shelf.panels.find(panel => panel.id === modelData.source) || ({})
                    name: modelData.name || ""
                    art: modelData.artwork_path || ""
                    covers: modelData.covers || []
                    origin: modelData.mixed ? ({chilpa: true}) : ({icon: panel.icon_image || panel.icon || ""})
                    owner: modelData.mixed ? "Chilpa" : modelData.provider || ""
                    onActivated: shelf.openPlaylist(modelData.id)
                    onPlayed: Playlists.play(modelData.id, false)
                }
            }
        }
    }
}
