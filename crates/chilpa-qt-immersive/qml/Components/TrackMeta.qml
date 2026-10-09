import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.ambient
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Primitives"
import "../Theme"

// The track's block under the cover, the same in Album Reactive, Static,
// Coverflow and the split's left column (03-immersive §1.1): "Now
// Playing" with the equalizer while it plays, title 28/700, artist 18,
// album 14 italic when there is one, and the quality chip 12 below. Every
// line carries a raised shadow, since the field under it can be as light
// as the cover.
Column {
    id: meta
    readonly property color accent: Ambient.lifted
    readonly property bool playing: Player.playing && !Player.paused
    spacing: Stage.metaApart

    Item {
        visible: meta.playing
        width: parent.width
        height: visible ? row.implicitHeight + Stage.metaNowBelow : 0
        Row {
            id: row
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Stage.menuApart
            height: Metrics.eqTall
            PlayingBars {
                tint: meta.accent
                active: Immersive.open
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                text: I18n.tr("Now Playing", I18n.rev)
                color: meta.accent
                font.pointSize: Stage.nowText
                font.weight: Type.semibold
                font.letterSpacing: Stage.nowTracking
                style: Text.Raised
                styleColor: Theme.color.immersiveTextShadow
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
    Text {
        width: parent.width
        text: Player.title
        color: Theme.color.immersiveTextStrong
        font.pointSize: Stage.metaTitleText
        font.weight: Type.bold
        style: Text.Raised
        styleColor: Theme.color.immersiveTextShadow
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
    }
    Text {
        width: parent.width
        text: Player.artist
        color: Theme.color.immersiveTextBody
        font.pointSize: Stage.metaArtistText
        style: Text.Raised
        styleColor: Theme.color.immersiveTextShadow
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
    }
    Text {
        visible: Player.album !== ""
        width: parent.width
        text: Player.album
        color: Theme.color.immersiveTextDim
        font.pointSize: Stage.metaAlbumText
        font.italic: true
        style: Text.Raised
        styleColor: Theme.color.immersiveTextShadow
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
    }
    Item {
        visible: Player.qualityLong !== ""
        width: parent.width
        height: visible ? chip.height + Stage.metaBadgeTop : 0
        QualityChip {
            id: chip
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            text: Player.qualityLong
        }
    }
}
