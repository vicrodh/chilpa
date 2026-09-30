import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The live player stays visible while another buffer is being edited.
Column {
    id: card
    spacing: Metrics.gap12
    Text {
        text: I18n.tr("Now playing", I18n.rev)
        color: Theme.color.textTertiary
        font.pointSize: Type.label
    }
    ArtTile {
        width: parent.width
        height: width
        source: Player.art
        radius: Metrics.radiusCard
    }
    Text {
        width: parent.width
        text: Player.title
        wrapMode: Text.Wrap
        font.pointSize: Type.titleSmall
        font.weight: Type.bold
        color: Theme.color.textPrimary
    }
    CreditLine { width: parent.width; credit: Player.artist; color: Theme.color.accent; font.pointSize: Type.meta }
    Text {
        width: parent.width
        text: Player.album
        wrapMode: Text.Wrap
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
    }
    SeekBar {
        width: parent.width
        height: Metrics.gap12
        position: Player.position
        duration: Player.duration
        onSeek: (seconds) => Player.seek(seconds)
    }
    Row {
        width: parent.width
        Text { width: parent.width / 2; text: Player.elapsed; font.pointSize: Type.small; color: Theme.color.textTertiary } // derived: half of the clock row
        Text { width: parent.width / 2; horizontalAlignment: Text.AlignRight; text: Player.remaining; font.pointSize: Type.small; color: Theme.color.textTertiary } // derived: half of the clock row
    }
    Text {
        width: parent.width
        text: Player.qualityLong
        wrapMode: Text.Wrap
        color: Theme.color.textSecondary
        font.pointSize: Type.small
    }
    Text {
        width: parent.width
        text: Cast.active ? Cast.destination : Player.device
        wrapMode: Text.Wrap
        color: Cast.active ? Theme.color.casting : Theme.color.textSecondary
        font.pointSize: Type.small
    }
}
