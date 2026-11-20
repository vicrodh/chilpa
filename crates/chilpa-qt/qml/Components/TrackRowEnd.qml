// hot-path: one per visible track, recycled while the list scrolls.
import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.offline
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/offline/qml/Primitives"

// What a track offers at its end, always in sight: the heart that makes it
// a favorite and the button that opens its menu, with the track's offline
// state around it. Each is its whole cell, as tall as the row, so the icon
// does not have to be aimed at. It has no mouse handling of its own: the
// list tells it which one the pointer is on and resolves the click.
Item {
    id: end

    property double trackId: 0
    property bool favorite: false
    property bool showFavorite: true
    property var columns: null
    property real favoriteWidth: showFavorite ? Metrics.rowAction : 0
    property bool inViewport: true
    readonly property bool busy: inViewport && Ratings.pending("track", trackId, Ratings.rev)
    // 1 the heart, 2 the menu button, 0 neither: the one under the pointer.
    property int hovered: 0

    readonly property bool canMark: inViewport && Ratings.markable("track", trackId, Ratings.rev)

    // The action at that point of the row's end.
    function actionAt(x, y) {
        if (!visible || y < 0 || y >= height)
            return 0
        if (x < 0 || x >= width) return 0
        return x < favoriteWidth ? (showFavorite && canMark ? 1 : 0) : 2
    }
    function act() {
        if (hovered === 1 && !busy)
            Ratings.setFavorite("track", trackId, !favorite)
    }
    // Where the menu opens, in `item`: under the button, ending with it.
    function menuPoint(item) {
        return end.mapToItem(item, width - Metrics.menuWidth, height + Metrics.gap4)
    }
    function said() {
        return hovered === 2 ? [I18n.tr("More", I18n.rev), Offline.tip(trackId, Offline.rev)].filter(text => text !== "").join("\n")
            : favorite ? I18n.tr("Remove from favorites", I18n.rev) : I18n.tr("Add to favorites", I18n.rev)
    }
    function explain() { if (hovered === 0) TipState.hide(end); else TipState.show(said(), end) }
    // Followed while the cell holds the tooltip: the offline state and the track change under a still pointer.
    readonly property string saying: hovered === 0 ? "" : said()
    onSayingChanged: if (TipState.target === end) explain()

    width: favoriteWidth + Metrics.rowAction
    onHoveredChanged: explain()
    onVisibleChanged: if (visible && hovered !== 0) explain() // TipState let it go when it hid
    onFavoriteChanged: if (hovered === 1) explain()

    Repeater {
        model: 2

        Item {
            id: cell
            required property int index
            readonly property bool heart: index === 0
            readonly property bool pointed: end.hovered === index + 1

            x: heart ? 0 : end.favoriteWidth
            width: heart ? end.favoriteWidth : Metrics.rowAction
            height: end.height
            visible: !heart || end.showFavorite && end.canMark

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
                active: cell.heart && cell.visible && end.busy && end.inViewport && end.visible && (!end.columns || end.columns.inView(end.x + cell.x + (cell.width - Metrics.iconSm) / 2, Metrics.iconSm)) // derived: the icon is centered in its cell
                sourceComponent: BusyMark { clock: Pulse; size: Metrics.iconSm; color: Theme.color.accent }
            }
        }
    }
    // The track's offline state, around the menu button.
    Loader {
        id: offline
        objectName: "trackOfflineMark"
        readonly property string phase: end.inViewport ? Offline.state(end.trackId, Offline.rev) : ""
        x: end.favoriteWidth + (Metrics.rowAction - width) / 2 // derived: centred on the menu's cell
        anchors.verticalCenter: parent.verticalCenter
        active: end.inViewport ? phase !== "" : item !== null
        sourceComponent: OfflineMark { clock: Pulse; phase: offline.phase; fraction: end.inViewport ? Offline.fraction(end.trackId, Offline.rev) : -1; inViewport: end.inViewport && (!end.columns || end.columns.inView(end.x + offline.x, offline.width)) }
    }
}
