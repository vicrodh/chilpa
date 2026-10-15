import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// A record opened under its row without leaving the grid (design/03
// AlbumQuickView): cover, kind and year, title, artist, the line of facts
// and its buttons on the left, its tracks on the right. A notch points at
// the card it came from.
Item {
    id: quick

    property int albumId
    // The center of the card it belongs to, from the left edge.
    property real notchX

    signal close()

    readonly property real notchHalf: Metrics.quickNotch * Math.SQRT2 / 2 // derived: half the turned square's diagonal

    implicitHeight: panel.y + panel.height

    AlbumDetailModel {
        id: detail
        albumId: quick.albumId
        contentRev: Library.contentRev + I18n.rev + (quick.albumId < 0 ? Library.remoteRev + Library.remoteArtRev : 0)
    }
    TrackListModel {
        id: tracks
        kind: "album"
        albumId: detail.selectedId
        albumPage: detail.tracksRev
        playingPath: Player.path
    }

    QueueMenu { id: queueMenu; objectName: "quickQueueMenu"; model: tracks; laterLabel: detail.title }

    Rectangle {
        id: panel
        readonly property var albumDetail: detail

        y: quick.notchHalf
        width: quick.width
        height: Math.max(left.height, right.height) + Metrics.quickPad + Metrics.quickPadBottom
        radius: Metrics.radiusPanel
        color: Theme.color.surfacePanel

        Edge {}

        Column {
            id: left

            x: Metrics.quickPad
            y: Metrics.quickPad
            width: Metrics.quickArt
            spacing: Metrics.gap6

            ArtTile {
                width: Metrics.quickArt
                height: Metrics.quickArt
                source: detail.art
            }
            Item { width: 1; height: Metrics.gap6 }
            Text {
                text: detail.kindLine
                font.pointSize: Type.micro
                font.weight: Type.semibold
                font.letterSpacing: Type.labelTracking
                color: Theme.color.textSecondary
            }
            Text {
                width: parent.width
                text: detail.title
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
                font.pointSize: Type.heroTitle
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            CreditLine {
                width: Math.min(implicitWidth, parent.width)
                credit: detail.artist; albumId: detail.selectedId
                font.pointSize: Type.body
                color: Theme.color.accent
            }
            Text {
                width: parent.width
                text: detail.meta
                wrapMode: Text.Wrap
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
            Item { width: 1; height: Metrics.gap6 }
            Row {
                spacing: Metrics.gap6

                PillButton {
                    kind: "primary"
                    glyph: Icons.playerPlayFilled
                    filled: true
                    text: I18n.tr("Play", I18n.rev)
                    onClicked: tracks.playAll(false)
                }
                IconButton {
                    round: true
                    glyph: Icons.arrowsShuffle
                    tip: I18n.tr("Shuffle", I18n.rev)
                    onClicked: tracks.playAll(true)
                }
                IconButton {
                    round: true
                    id: queueButton
                    objectName: "quickQueueActions"
                    glyph: Lucide.listEnd
                    iconFamily: Lucide.font
                    tip: I18n.tr("Queue actions", I18n.rev)
                    enabled: tracks.count > 0
                    onClicked: { const at = queueButton.mapToItem(quick, 0, queueButton.height + Metrics.gap4); queueMenu.show(at.x, at.y) }
                }
                IconButton {
                    round: true
                    glyph: Icons.browserMaximize
                    tip: I18n.tr("Open album", I18n.rev)
                    onClicked: Navigation.go("album", detail.selectedId)
                }
            }
            AlbumVersionSelect {
                width: parent.width
                detail: panel.albumDetail
            }
            RatingStars {
                value: Ratings.stars("album", detail.selectedId, Ratings.rev)
                interactive: detail.selectedId > 0
                onRated: (stars) => Ratings.rate("album", detail.selectedId, stars)
            }
        }

        Column {
            id: right

            x: left.x + left.width + Metrics.quickGap
            y: Metrics.quickPad
            width: panel.width - x - Metrics.quickPad
            spacing: Metrics.gap8

            Item {
                width: parent.width
                height: Metrics.button24

                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: Metrics.gap16

                    Row {
                        visible: detail.discs !== ""
                        spacing: Metrics.gap6
                        Icon { glyph: Icons.disc; size: Metrics.iconSm }
                        Text { text: detail.discs; font.pointSize: Type.meta; color: Theme.color.textSecondary }
                    }
                    Row {
                        spacing: Metrics.gap6
                        Icon { glyph: Icons.infoCircle; size: Metrics.iconSm; color: Theme.color.textTertiary }
                        Text { text: detail.info; font.pointSize: Type.meta; color: Theme.color.textTertiary }
                    }
                }
                IconButton {
                    anchors.right: parent.right
                    size: Metrics.button24
                    glyph: Icons.x
                    tip: I18n.tr("Close", I18n.rev)
                    onClicked: quick.close()
                }
            }

            TrackList {
                width: parent.width
                height: implicitHeight
                fitContent: true
                showArtist: false
                showAlbum: false
                showArt: false
                model: tracks
            }
        }
    }

    // Half a turned square over the panel's edge, filled like the panel so
    // it covers the hairline where they meet.
    Item {
        x: quick.notchX - width / 2
        width: quick.notchHalf * 2 // derived: the whole diagonal
        height: quick.notchHalf + 1
        clip: true

        Rectangle {
            anchors.centerIn: parent
            anchors.verticalCenterOffset: parent.height / 2 - 1 // derived: centered on the panel's edge
            width: Metrics.quickNotch
            height: Metrics.quickNotch
            rotation: 45
            color: Theme.color.surfacePanel

            Edge {}
        }
    }
}
