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
    property bool work: of ? of.headerWork : false
    property string label: of ? of.headerLabel : ""
    property string composer: of ? of.headerComposer : ""
    property string composerTarget: of ? of.headerComposerTarget : ""
    property string artist: of ? of.artist : ""
    property string meta: of ? of.headerMeta : ""
    property url art: of ? of.art : ""
    property bool discActions: false
    property int actionHovered: 0
    // Colors the cover's place while it has none.
    property int tint: of ? of.index : 0

    readonly property bool covered: art.toString() !== ""
    readonly property Item composerLabel: composerName
    readonly property Item actionArea: discButtons

    implicitHeight: record ? Metrics.rowRecord : work ? Metrics.rowWork : Metrics.rowGroup

    Row {
        visible: !head.record && !head.work
        width: parent.width - (head.discActions ? discButtons.width + Metrics.gap12 : 0)
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
            width: Math.min(implicitWidth, Math.max(0, parent.width - Metrics.artGroup - metaText.implicitWidth - Metrics.gap24))
            text: head.label
            elide: Text.ElideRight
            font.pointSize: Type.meta
            font.weight: Type.semibold
            color: Theme.color.textPrimary
        }
        Text {
            id: metaText
            anchors.verticalCenter: parent.verticalCenter
            text: head.meta
            font.pointSize: Type.meta
            color: Theme.color.textTertiary
        }
    }

    Row {
        id: discButtons
        visible: head.discActions
        x: head.width - width - Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap4
        Repeater {
            model: [Icons.playerPlayFilled, Icons.arrowsShuffle, Icons.playlistAdd]
            Rectangle {
                required property int index
                required property string modelData
                width: Metrics.button30
                height: Metrics.button30
                radius: Metrics.button30
                color: head.actionHovered === index + 1 ? Theme.color.stateHover : Theme.color.fillField
                Icon {
                    anchors.centerIn: parent
                    glyph: modelData
                    filled: index === 0
                    size: Metrics.iconMd
                    color: Theme.color.textPrimary
                }
            }
        }
    }

    Row {
        visible: head.work && !head.record
        x: Metrics.gap12
        y: Metrics.gap14
        width: parent.width - Metrics.gap12
        height: Metrics.rowWork - Metrics.gap14 - Metrics.gap4
        spacing: 0
        Text {
            width: Math.min(implicitWidth, Math.max(0, parent.width - (head.composer !== "" ? prefix.implicitWidth + composerName.implicitWidth + suffix.implicitWidth : 0)))
            text: head.label
            elide: Text.ElideRight
            font.pointSize: Type.body
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }
        Text {
            id: prefix
            visible: head.composer !== ""
            text: "("
            leftPadding: Metrics.gap4
            font.pointSize: Type.body
            font.weight: Type.bold
            color: Theme.color.textSecondary
        }
        Text {
            id: composerName
            visible: head.composer !== ""
            text: head.composer
            font.pointSize: Type.body
            font.weight: Type.bold
            color: head.composerTarget !== "" ? Theme.color.accent : Theme.color.textSecondary
        }
        Text {
            id: suffix
            visible: head.composer !== ""
            text: ")"
            font.pointSize: Type.body
            font.weight: Type.bold
            color: Theme.color.textSecondary
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
