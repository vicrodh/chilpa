import QtQuick
import "../Theme"
import "../Primitives"

// What stands before a group of rows in a track list (design/03 TrackRow).
// As a divider: the group's own cover when it has one, a disc when it has
// not, its label and how many follow. As a record among several, or one
// disc of a box: its cover, its title, whose it is and a line about it.
Item {
    id: head

    // The row of a track list it is the header of, which it reads itself
    // from; or nothing, and each part is given.
    property var of: null
    property bool record: of ? of.record : false
    property string label: of ? of.headerLabel : ""
    property string artist: of ? of.artist : ""
    property string meta: of ? of.headerMeta : ""
    property url art: of ? of.art : ""
    // Colors the cover's place while it has none.
    property int tint: of ? of.index : 0

    readonly property bool covered: art.toString() !== ""

    implicitHeight: record ? Metrics.rowRecord : Metrics.rowGroup

    Row {
        visible: !head.record
        width: parent.width
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap8

        Loader {
            active: !head.record && head.covered
            visible: active
            anchors.verticalCenter: parent.verticalCenter
            sourceComponent: ArtTile {
                width: Metrics.artGroup
                height: Metrics.artGroup
                radius: Metrics.radiusArtSmall
                source: head.art
                tint: head.tint
            }
        }
        Icon {
            visible: !head.covered
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.disc
            size: Metrics.iconMd
            color: Theme.color.accent
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: head.label
            font.pointSize: Type.meta
            font.weight: Type.semibold
            color: Theme.color.textPrimary
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: head.meta
            font.pointSize: Type.meta
            color: Theme.color.textTertiary
        }
    }

    Row {
        visible: head.record
        width: parent.width
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap12

        Loader {
            id: cover
            active: head.record
            visible: active
            width: Metrics.artRecord
            height: Metrics.artRecord
            sourceComponent: ArtTile {
                radius: Metrics.radiusArtSmall
                source: head.art
                tint: head.tint
            }
        }
        Column {
            width: parent.width - cover.width - parent.spacing
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap2

            Text {
                width: parent.width
                text: head.label
                elide: Text.ElideRight
                font.pointSize: Type.body
                font.weight: Type.semibold
                color: Theme.color.textPrimary
            }
            Text {
                width: parent.width
                visible: text !== ""
                text: head.artist
                elide: Text.ElideRight
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
            Text {
                width: parent.width
                visible: text !== ""
                text: head.meta
                elide: Text.ElideRight
                font.pointSize: Type.meta
                color: Theme.color.textTertiary
            }
        }
    }
}
