import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// What the row under the pointer offers in a playlist in its own order:
// moving it up or down. One for the list, over whichever row is hovered,
// and none per row. The heart and the menu button are the row's own, always
// in sight.
Row {
    id: root

    property int row: -1
    property TrackListModel model

    spacing: Metrics.gap2

    IconButton {
        size: Metrics.button24
        glyph: Icons.chevronUp
        tip: I18n.tr("Move up", I18n.rev)
        onClicked: root.model.moveInPlaylist(root.row, -1)
    }
    IconButton {
        size: Metrics.button24
        glyph: Icons.chevronDown
        tip: I18n.tr("Move down", I18n.rev)
        onClicked: root.model.moveInPlaylist(root.row, 1)
    }
}
