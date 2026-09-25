import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// A record (design/06 Album): its header, the bar with discs and where the
// information comes from, and its tracks with a header per disc.
Flickable {
    id: page

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}

    AlbumDetailModel {
        id: detail
        albumId: Navigation.routeId
    }
    TrackListModel {
        id: tracks
        kind: "album"
        albumId: Navigation.routeId
        playingPath: Player.path
    }

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        spacing: Metrics.gap16

        AlbumHero {
            width: parent.width
            detail: detail
            onPlay: tracks.playAll(false)
            onShuffle: tracks.playAll(true)
        }

        Rectangle { width: parent.width; height: 1; color: Theme.color.strokeHairline }

        Item {
            width: parent.width
            height: Metrics.gap16

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

            Text {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: detail.summary
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
        }

        Rectangle { width: parent.width; height: 1; color: Theme.color.strokeHairline }

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
