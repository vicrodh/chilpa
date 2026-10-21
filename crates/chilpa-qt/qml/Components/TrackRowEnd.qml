// hot-path: one per visible track, recycled while the list scrolls.
import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// What a track offers at its end, always in sight: the heart that makes it
// a favorite and the button that opens its menu. Each is its whole cell, as
// tall as the row, so the icon does not have to be aimed at. It has no
// mouse handling of its own: the list tells it which one the pointer is on
// and resolves the click.
Item {
    id: end

    property double trackId: 0
    property bool favorite: false
    property bool inViewport: true
    readonly property bool busy: Ratings.pending("track", trackId, Ratings.rev)
    // 1 the heart, 2 the menu button, 0 neither: the one under the pointer.
    property int hovered: 0

    readonly property bool canMark: Ratings.markable("track", trackId, Ratings.rev)

    // The action at that point of the row's end.
    function actionAt(x, y) {
        if (!visible || y < 0 || y >= height)
            return 0
        const cell = Math.floor(x / Metrics.rowAction)
        return cell === 0 && canMark ? 1 : cell === 1 ? 2 : 0
    }
    function act() {
        if (hovered === 1 && !busy)
            Ratings.setFavorite("track", trackId, !favorite)
    }
    // Where the menu opens, in `item`: under the button, ending with it.
    function menuPoint(item) {
        return end.mapToItem(item, width - Metrics.menuWidth, height + Metrics.gap4)
    }
    function explain() {
        if (hovered === 0)
            TipState.hide(end)
        else
            TipState.show(hovered === 2 ? I18n.tr("More", I18n.rev)
                : favorite ? I18n.tr("Remove from favorites", I18n.rev)
                : I18n.tr("Add to favorites", I18n.rev), end)
    }

    width: Metrics.colMore
    onHoveredChanged: explain()
    onFavoriteChanged: if (hovered === 1) explain()

    Repeater {
        model: 2

        Item {
            id: cell
            required property int index
            readonly property bool heart: index === 0
            readonly property bool pointed: end.hovered === index + 1

            x: index * Metrics.rowAction
            width: Metrics.rowAction
            height: end.height
            visible: !heart || end.canMark

            Rectangle {
                anchors.centerIn: parent
                width: Metrics.button24
                height: Math.min(width, parent.height)
                radius: Metrics.radiusItem
                visible: cell.pointed
                color: Theme.color.stateHover
            }
            Icon {
                visible: !cell.heart || !end.busy
                anchors.centerIn: parent
                glyph: !cell.heart ? Icons.dots : end.favorite ? Icons.heartFilled : Icons.heart
                filled: cell.heart && end.favorite
                size: Metrics.iconSm
                color: cell.heart && end.favorite ? Theme.color.favorite
                     : cell.pointed ? Theme.color.textPrimary : Theme.color.textTertiary
            }
            Loader {
                anchors.centerIn: parent
                active: cell.heart && end.busy && end.inViewport && end.visible
                sourceComponent: BusyMark { clock: Pulse; size: Metrics.iconSm; color: Theme.color.accent }
            }
        }
    }
}
