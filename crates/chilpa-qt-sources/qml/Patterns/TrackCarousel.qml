import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// I reuse the track model's playback, navigation and menu in a single card row.
Column {
    id: shelf
    property TrackListModel model
    property Flickable viewport: null
    property string title: ""
    property string sourceIcon: ""
    property bool showPlays: false
    readonly property alias scrollView: rail
    readonly property real farthest: Math.max(0, rail.contentWidth - rail.width)
    readonly property real step: CardLayout.columns(width) * (CardLayout.cardWidth + Metrics.gridGap) // derived: a page of track cards
    spacing: Metrics.gap8
    SectionHeader {
        width: parent.width; visible: !!shelf.title
        title: shelf.title; sourceIcon: shelf.sourceIcon
        paged: shelf.farthest > 0
        canBack: rail.contentX > 1; canForward: rail.contentX < shelf.farthest - 1
        onBack: rail.contentX = Math.max(0, rail.contentX - shelf.step)
        onForward: rail.contentX = Math.min(shelf.farthest, rail.contentX + shelf.step)
    }
    MotionListView {
        id: rail
        motionViewport: shelf.viewport
        objectName: "trackCarouselRail"
        width: parent.width; height: CardLayout.cardHeight + Metrics.gap8
        contentHeight: height
        orientation: ListView.Horizontal
        spacing: Metrics.gridGap
        model: shelf.model
        interactive: contentWidth > width
        cacheBuffer: width
        reuseItems: true; clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.horizontal: ViewportScrollBar {}
        delegate: Item {
            id: card
            required property int index
            required property var model
            readonly property bool available: model.available !== false
            width: CardLayout.cardWidth; height: CardLayout.cardHeight
            readonly property real motionOpacity: available ? 1 : Metrics.disabledOpacity
            opacity: motionOpacity
            ArtTile { id: cover; width: parent.width; height: width; source: card.model.art || ""; tint: card.index }
            TapHandler {
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onTapped: (eventPoint, button) => {
                    if (!card.available) return
                    const at = artist.mapFromItem(card, eventPoint.position.x, eventPoint.position.y)
                    if (button === Qt.LeftButton && artist.navigable && at.x >= 0 && at.x <= artist.width && at.y >= 0 && at.y <= artist.height) return
                    if (button === Qt.RightButton) {
                        shelf.model.select(card.index, 4)
                        const point = card.mapToItem(shelf, eventPoint.position.x, eventPoint.position.y)
                        menu.show(card.index, point.x, point.y)
                    } else shelf.model.play(card.index)
                }
            }
            HoverHandler { id: hover; cursorShape: card.available ? Qt.PointingHandCursor : Qt.ArrowCursor }
            SourceIcon {
                z: 1
                anchors.right: cover.right; anchors.bottom: cover.bottom; anchors.margins: Metrics.overlayPad
                sourceInfo: JSON.parse(card.model.sourceInfo || "{}")
                width: Metrics.iconSm; height: Metrics.iconSm
            }
            Loader {
                anchors.fill: cover
                active: hover.hovered && card.available
                sourceComponent: Rectangle {
                    color: Theme.color.overlayScrim; radius: Metrics.radiusCard
                    Text {
                        anchors.top: parent.top; anchors.left: parent.left; anchors.margins: Metrics.overlayPad
                        text: shelf.showPlays ? I18n.tr("Plays", I18n.rev) + ": " + card.model.plays : card.model.duration || ""
                        color: Theme.color.backdropText; font.pointSize: Type.meta
                    }
                    Item {
                        anchors.left: parent.left; anchors.right: parent.right
                        anchors.bottom: parent.bottom; anchors.margins: Metrics.overlayPad
                        anchors.bottomMargin: Metrics.overlayPad + Metrics.iconSm + Metrics.gap8
                        height: Metrics.overlayPlay
                        IconButton {
                            objectName: "trackCarouselPlay"
                            round: true; tone: "overlay"; size: Metrics.overlayPlay
                            filled: true
                            glyph: card.model.playing && !Player.paused ? Icons.playerPauseFilled : Icons.playerPlayFilled
                            tip: card.model.playing && !Player.paused ? I18n.tr("Pause", I18n.rev) : I18n.tr("Play", I18n.rev)
                            onClicked: card.model.playing ? Player.toggle() : shelf.model.play(card.index)
                        }
                        Row {
                            anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                            spacing: Metrics.gap6
                            FavoriteButton {
                                clock: Pulse
                                objectName: "trackCarouselFavorite"
                                anchors.verticalCenter: parent.verticalCenter
                                visible: Ratings.markable("track", card.model.trackId, Ratings.rev)
                                round: true; tone: "overlay"; size: Metrics.overlayButton
                                readonly property bool favorite: Ratings.favorite("track", card.model.trackId, Ratings.rev)
                                filled: favorite; checked: favorite; heart: true; iconSize: Metrics.overlayIcon
                                busy: Ratings.pending("track", card.model.trackId, Ratings.rev)
                                glyph: favorite ? Icons.heartFilled : Icons.heart
                                tip: favorite ? I18n.tr("Remove from favorites", I18n.rev) : I18n.tr("Add to favorites", I18n.rev)
                                onClicked: Ratings.setFavorite("track", card.model.trackId, !favorite)
                            }
                            IconButton {
                                anchors.verticalCenter: parent.verticalCenter
                                round: true; tone: "overlay"; size: Metrics.overlayButton; iconSize: Metrics.overlayIcon; glyph: Icons.dots
                                tip: I18n.tr("More", I18n.rev)
                                onClicked: {
                                    shelf.model.select(card.index, 4)
                                    const point = mapToItem(shelf, 0, height)
                                    menu.show(card.index, point.x, point.y)
                                }
                            }
                        }
                    }
                }
            }
            Text {
                anchors.top: cover.bottom; anchors.topMargin: Metrics.cardTextGap
                width: parent.width; text: card.model.title || ""; elide: Text.ElideRight
                color: card.model.playing ? Theme.color.accent : Theme.color.textPrimary
                font.pointSize: Type.body
            }
            Text {
                id: artist
                objectName: "trackCarouselArtist"
                anchors.bottom: parent.bottom; width: parent.width
                text: card.model.artist || ""; elide: Text.ElideRight; font.pointSize: Type.meta
                color: artistHover.hovered ? Theme.color.accentHover : Theme.color.textSecondary
                readonly property bool navigable: card.available && shelf.model.canGo(card.index, "artist")
                HoverHandler { id: artistHover; cursorShape: artist.navigable ? Qt.PointingHandCursor : Qt.ArrowCursor }
                MouseArea {
                    anchors.fill: parent; enabled: artist.navigable
                    cursorShape: Qt.PointingHandCursor
                    onClicked: shelf.model.go(card.index, "artist")
                }
            }
        }
    }
    TrackMenu { id: menu; model: shelf.model }
}
