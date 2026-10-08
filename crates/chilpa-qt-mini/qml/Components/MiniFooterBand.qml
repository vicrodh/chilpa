import QtQuick
import app.chilpa
import app.chilpa.mini
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The footer at full and compact scale (02-mini §1.4): the seek bar across
// the top, the times under it, and the controls row with the volume at
// the left, the transport in the middle and the capsule at the right. The
// transport is centered by an invisible box the width of the volume button
// at the right end, the phantom: drop it and the transport drifts right
// by half a button. QBZ says so in its own comment.
Item {
    id: band

    // 0 full, 1 compact: the index into the card's pairs.
    property int tier: 0
    property bool lightForeground: false
    property real progress: 0
    property var hostWindow: null
    property bool windowHovered: false
    property bool hasTrack: false
    property bool canPlay: false

    signal preview(real fraction)
    signal commit(real fraction)

    MiniSeek {
        id: seek
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: Card.seekHeight[band.tier]
        trackHeight: Card.seekTrack[band.tier]
        progress: band.progress
        lightForeground: band.lightForeground
        onPreview: (fraction) => band.preview(fraction)
        onCommit: (fraction) => band.commit(fraction)
    }

    // The times take what the seek bar and the controls leave.
    Item {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: seek.bottom
        anchors.bottom: controls.top

        Text {
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            text: Player.elapsed
            color: band.lightForeground ? Theme.color.miniOnBackdrop : Theme.color.textSecondary
            font.pointSize: Card.timeSize[band.tier]
        }
        Text {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            text: Player.remaining
            color: band.lightForeground ? Theme.color.miniOnBackdrop : Theme.color.textSecondary
            font.pointSize: Card.timeSize[band.tier]
        }
    }

    Item {
        id: controls
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: Card.controls[band.tier]

        MiniVolume {
            id: volume
            // Above the transport, so its pill and its dismiss layer are
            // not painted under the buttons they must cover.
            z: 2
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            side: Card.button[band.tier]
            iconSize: Card.iconVolume[band.tier]
            lightForeground: band.lightForeground
        }
        // The phantom: the volume button's width, at the other end.
        Item {
            id: phantom
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: Card.button[band.tier]
            height: Card.button[band.tier]
        }
        MiniTransport {
            anchors.verticalCenter: parent.verticalCenter
            x: volume.x + volume.width + Math.round((phantom.x - volume.x - volume.width - width) / 2) // derived: centered between the volume and the phantom
            side: Card.button[band.tier]
            spacing: Card.gap[band.tier]
            iconShuffle: Card.iconShuffle[band.tier]
            iconSkip: Card.iconSkip[band.tier]
            iconPlay: Card.iconPlay[band.tier]
            able: band.hasTrack
            playAble: band.canPlay
            lightForeground: band.lightForeground
        }
        // The capsule, over everything including the pill's dismiss layer.
        Loader {
            z: 3
            active: Mini.open
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            sourceComponent: MiniCapsule {
                windowHovered: band.windowHovered
                activeSurface: Mini.surface
                hostWindow: band.hostWindow
            }
        }
    }
}
