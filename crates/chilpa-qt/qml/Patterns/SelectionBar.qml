import QtQuick
import QtQuick.Effects
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "../Components"

// What can be done with the records selected (design/03 SelectionBar):
// how many, play, queue, play next, add to a playlist, and the way out. It
// floats over the content above the player. Esc, or going somewhere else,
// lets the selection go.
Item {
    id: bar

    implicitWidth: row.implicitWidth + Metrics.gap16 * 2 // derived: padding on both sides
    implicitHeight: Metrics.selectionBarHeight

    // Clicks in its empty space do not reach the grid under it.
    // no cursor: keeps clicks off the list beneath
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
    }

    RectangularShadow {
        anchors.fill: panel
        radius: panel.radius
        offset.y: Metrics.shadowElevatedY
        blur: Metrics.shadowElevatedBlur
        color: Theme.color.shadowElevated
    }
    RectangularShadow {
        anchors.fill: panel
        radius: panel.radius
        offset.y: Metrics.shadowKeyY
        blur: Metrics.shadowKeyBlur
        color: Theme.color.shadowElevatedKey
    }

    Rectangle {
        id: panel
        anchors.fill: parent
        radius: Metrics.radiusPanel
        color: Theme.color.surfaceRaised

        Edge {}
    }

    Row {
        id: row
        anchors.centerIn: parent
        spacing: Metrics.gap6

        Text {
            anchors.verticalCenter: parent.verticalCenter
            rightPadding: Metrics.gap10
            text: I18n.trCount("{} record", "{} records", AlbumActions.selectedCount, I18n.rev)
            font.pointSize: Type.body
            font.weight: Type.semibold
            color: Theme.color.textPrimary
        }
        IconButton {
            glyph: Icons.playerPlayFilled
            filled: true
            tone: "accent"
            tip: I18n.tr("Play", I18n.rev)
            onClicked: AlbumActions.play(0, false)
        }
        IconButton {
            glyph: Lucide.listEnd
            iconFamily: Lucide.font
            tip: I18n.tr("Add to queue", I18n.rev)
            onClicked: AlbumActions.queue(0)
        }
        IconButton {
            glyph: Lucide.listStart
            iconFamily: Lucide.font
            tip: I18n.tr("Play next", I18n.rev)
            onClicked: AlbumActions.playNext(0)
        }
        IconButton {
            id: add
            glyph: Icons.playlistAdd
            tip: I18n.tr("Add to playlist…", I18n.rev)
            onClicked: {
                picker.trackIdsJson = AlbumActions.trackIdsJson(0)
                picker.show(add.x + row.x, 0)
                picker.y = -picker.height - Metrics.gap8
            }
        }
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: 1
            height: Metrics.iconNav
            color: Theme.color.strokeHairline
        }
        IconButton {
            glyph: Icons.x
            tip: I18n.tr("Clear the selection", I18n.rev)
            onClicked: AlbumActions.clearSelection()
        }
    }

    PlaylistPicker {
        id: picker
    }

    Shortcut {
        sequence: "Escape"
        enabled: bar.visible
        onActivated: AlbumActions.clearSelection()
    }

    Connections {
        target: Navigation
        function onScreenChanged() { AlbumActions.clearSelection() }
        function onRouteIdChanged() { AlbumActions.clearSelection() }
        function onRouteKeyChanged() { AlbumActions.clearSelection() }
        function onActiveChanged() { AlbumActions.clearSelection() }
    }
}
