import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// What the row under the pointer offers at its end: in a playlist in its
// own order, moving it; the heart that makes it a favorite; and the button
// that opens its menu, for who does not think of the right click. One for
// the list, over whichever row is hovered, and none per row.
Row {
    id: root

    property int row: -1
    property var item
    property TrackListModel model
    property bool reorderable: false

    readonly property bool marks: item ? item.trackId > 0 : false
    readonly property bool favorite: marks && Ratings.favorite("track", item.trackId, Ratings.rev)

    signal menu(int row, real left, real top)

    spacing: Metrics.gap2

    IconButton {
        visible: root.reorderable
        size: Metrics.button24
        glyph: Icons.chevronUp
        tip: I18n.tr("Move up", I18n.rev)
        onClicked: root.model.moveInPlaylist(root.row, -1)
    }
    IconButton {
        visible: root.reorderable
        size: Metrics.button24
        glyph: Icons.chevronDown
        tip: I18n.tr("Move down", I18n.rev)
        onClicked: root.model.moveInPlaylist(root.row, 1)
    }
    IconButton {
        visible: root.marks
        size: Metrics.button24
        glyph: root.favorite ? Icons.heartFilled : Icons.heart
        filled: root.favorite
        tone: root.favorite ? "accent" : "secondary"
        tip: root.favorite ? I18n.tr("Remove from favorites", I18n.rev)
                           : I18n.tr("Add to favorites", I18n.rev)
        onClicked: Ratings.setFavorite("track", root.item.trackId, !root.favorite)
    }
    IconButton {
        id: more
        size: Metrics.button24
        glyph: Icons.dots
        tip: I18n.tr("More", I18n.rev)
        onClicked: {
            const at = more.mapToItem(root.parent, more.width - Metrics.menuWidth, more.height + Metrics.gap4)
            root.menu(root.row, at.x, at.y)
        }
    }
}
