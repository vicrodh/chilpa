import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.mini
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The footer at micro scale (02-mini §1.4), where the footer is the whole
// card: one elided line of title and artist that is also the drag handle,
// sixteen-pixel controls centered with no phantom, since there is no
// volume here, and a hairline seek with square ends. The capsule sits at
// the right, centered on the card rather than on the header line, where
// QBZ's own arithmetic left it high.
Item {
    id: micro

    property bool lightForeground: false
    property real progress: 0
    property var hostWindow: null
    property bool windowHovered: false
    property bool hasTrack: false
    property bool canPlay: false

    signal preview(real fraction)
    signal commit(real fraction)

    Item {
        id: header
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: Card.microHeader

        Text {
            anchors.fill: parent
            // A literal " - ", as QBZ joins them.
            text: Player.path !== ""
                ? (Player.artist !== "" ? Player.title + " - " + Player.artist : Player.title)
                : I18n.tr("No track playing", I18n.rev)
            color: micro.lightForeground ? Theme.color.miniOnBackdrop : Theme.color.textSecondary
            font.pointSize: Card.microText
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
        // no cursor: the header is the drag handle, not a button
        MouseArea {
            anchors.fill: parent
            onPressed: if (micro.hostWindow) micro.hostWindow.startSystemMove()
        }
        Loader {
            z: 1
            active: Mini.open
            anchors.right: parent.right
            anchors.rightMargin: Card.microPad
            y: Math.round((micro.height - Card.microTop) / 2 - height / 2) // derived: centered on the card, which starts a hairline above this item
            sourceComponent: MiniCapsule {
                windowHovered: micro.windowHovered
                activeSurface: Mini.surface
                hostWindow: micro.hostWindow
            }
        }
    }

    Item {
        id: strip
        anchors.top: header.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: Card.microControls

        MiniTransport {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.verticalCenter: parent.verticalCenter
            side: Card.microButton
            spacing: Card.microBetween
            iconShuffle: Card.microIconShuffle
            iconSkip: Card.microIconSkip
            iconPlay: Card.microIconPlay
            able: micro.hasTrack
            playAble: micro.canPlay
            lightForeground: micro.lightForeground
        }
    }

    MiniSeek {
        anchors.top: strip.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: Card.microSeek
        trackHeight: Card.microSeekTrack
        rounded: false
        progress: micro.progress
        lightForeground: micro.lightForeground
        onPreview: (fraction) => micro.preview(fraction)
        onCommit: (fraction) => micro.commit(fraction)
    }
}
