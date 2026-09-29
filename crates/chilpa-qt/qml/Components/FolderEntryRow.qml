// hot-path: one per subfolder, recycled while a folder with thousands scrolls.
import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// A subfolder: a cover from one of its records (or a folder glyph), its
// name, how many tracks it holds, and the way in.
Item {
    id: row

    required property int index
    required property string name
    required property string path
    required property int count
    required property int folders
    required property double album
    required property url art

    signal open()
    signal context(real left, real top)

    height: Metrics.folderEntry

    Rectangle {
        anchors.fill: parent
        radius: Metrics.radiusControl
        color: hover.hovered ? Theme.color.stateHover : "transparent"
    }

    Item {
        id: lead
        x: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.folderArt
        height: Metrics.folderArt

        ArtTile {
            anchors.fill: parent
            visible: row.art.toString() !== ""
            radius: Metrics.radiusArtSmall
            source: row.art
            tint: row.index
        }
        Icon {
            anchors.centerIn: parent
            visible: row.art.toString() === ""
            glyph: Icons.folder
            size: Metrics.iconNav
        }
    }

    Text {
        anchors.left: lead.right
        anchors.leftMargin: Metrics.gap12
        anchors.right: meta.left
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        text: row.name
        elide: Text.ElideMiddle
        font.pointSize: Type.body
        color: Theme.color.textPrimary
    }

    Text {
        id: meta
        anchors.right: chevron.left
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        visible: row.count > 0
        text: (row.folders > 0 ? I18n.trCount("{} folder", "{} folders", row.folders, I18n.rev) + " · " : "")
              + I18n.trCount("{} track", "{} tracks", row.count, I18n.rev)
        font.pointSize: Type.small
        color: Theme.color.textTertiary
    }

    Icon {
        id: chevron
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.chevronRight
        size: Metrics.iconSm
        color: Theme.color.textTertiary
    }

    HoverHandler { id: hover }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: row.open() }
    TapHandler { acceptedButtons: Qt.RightButton; onTapped: (point) => row.context(point.position.x, point.position.y) }
}
