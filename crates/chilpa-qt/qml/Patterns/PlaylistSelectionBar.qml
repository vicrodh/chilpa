import QtQuick
import QtQuick.Effects
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// Bulk actions for the opt-in selection mode on a playlist.
Item {
    id: bar

    property TrackListModel model
    property string playlistName
    property bool canRemove: true
    signal finished()

    implicitWidth: actions.implicitWidth + Metrics.selectionHorizontalPadding
    implicitHeight: Metrics.selectionBarHeight

    RectangularShadow {
        anchors.fill: panel
        radius: panel.radius
        offset.y: Metrics.shadowElevatedY
        blur: Metrics.shadowElevatedBlur
        color: Theme.color.shadowElevated
    }
    Rectangle {
        id: panel
        anchors.fill: parent
        radius: Metrics.radiusPanel
        color: Theme.color.surfaceRaised
        Edge {}
    }
    Row {
        id: actions
        anchors.centerIn: parent
        spacing: Metrics.gap6

        Text {
            anchors.verticalCenter: parent.verticalCenter
            rightPadding: Metrics.gap10
            text: I18n.trCount("{} track", "{} tracks", bar.model.selectedCount, I18n.rev)
            font.pointSize: Type.body
            font.weight: Type.semibold
            color: Theme.color.textPrimary
        }
        IconButton {
            glyph: Icons.heart
            tip: I18n.tr("Add to favorites", I18n.rev)
            onClicked: bar.model.favoriteChosen(bar.model.selectedAnyRow(), true)
        }
        IconButton {
            glyph: Icons.listDetails
            tip: I18n.tr("Add to queue", I18n.rev)
            onClicked: bar.model.queue(bar.model.selectedAnyRow())
        }
        IconButton {
            glyph: Icons.playerTrackNext
            tip: I18n.tr("Play next", I18n.rev)
            onClicked: bar.model.playNext(bar.model.selectedAnyRow())
        }
        IconButton {
            glyph: Icons.alarm
            tip: I18n.tr("Play later", I18n.rev)
            onClicked: bar.model.playLater(bar.model.selectedAnyRow(), bar.playlistName)
        }
        IconButton {
            id: add
            glyph: Icons.playlistAdd
            tip: I18n.tr("Add to playlist…", I18n.rev)
            onClicked: {
                picker.trackIds = bar.model.selectedTrackIds()
                picker.show(add.x, 0)
            }
        }
        IconButton {
            visible: bar.canRemove
            glyph: Icons.trash
            tip: I18n.tr("Remove from playlist", I18n.rev)
            onClicked: bar.model.removeChosen(bar.model.selectedAnyRow())
        }
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: 1
            height: Metrics.iconNav
            color: Theme.color.strokeHairline
        }
        IconButton {
            glyph: Icons.x
            tip: I18n.tr("Finish selecting", I18n.rev)
            onClicked: bar.finished()
        }
    }
    PlaylistPicker { id: picker }
}
