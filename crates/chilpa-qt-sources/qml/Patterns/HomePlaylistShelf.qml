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
                Column {
                    required property var modelData
                    width: CardLayout.cardWidth; spacing: Metrics.gap4
                    height: CardLayout.cardHeight
                    Item {
                        width: parent.width; height: width
                        ArtTile {
                            id: cover; anchors.fill: parent
                            source: modelData.artwork_path ? modelData.artwork_path.indexOf("://") >= 0
                                ? modelData.artwork_path
                                : "file://" + modelData.artwork_path.split("/").map(encodeURIComponent).join("/") : ""
                        }
                        PlaylistCollage {
                            anchors.fill: parent; visible: !cover.ready && (modelData.covers || []).length > 0
                            covers: modelData.covers || []
                        }
                        Icon {
                            anchors.centerIn: parent; visible: !cover.ready && !(modelData.covers || []).length
                            glyph: Icons.playlist; size: Metrics.iconPlay; color: Theme.color.textSecondary
                        }
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: shelf.openPlaylist(modelData.id) }
                        IconButton {
                            anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.margins: Metrics.gap8
                            round: true; glyph: Icons.playerPlayFilled; filled: true
                            tip: I18n.tr("Play", I18n.rev)
                            onClicked: Playlists.play(modelData.id, false)
                        }
                    }
                    Text { width: parent.width; text: modelData.name || ""; elide: Text.ElideRight; font.pointSize: Type.body; color: Theme.color.textPrimary }
                    Row {
                        id: sourceLabel
                        width: parent.width; spacing: Metrics.gap4
                        SourceIcon {
                            id: providerIcon
                            readonly property var panel: shelf.panels.find(panel => panel.id === modelData.source) || ({})
                            visible: !!modelData.source
                            sourceInfo: ({icon: panel.icon_image || panel.icon || ""})
                            width: visible ? Metrics.iconSm : 0; height: Metrics.iconSm
                        }
                        Text { width: sourceLabel.width - providerIcon.width - sourceLabel.spacing; text: modelData.mixed ? "Chilpa" : modelData.provider || ""; elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary }
                    }
                }
            }
        }
    }
}
