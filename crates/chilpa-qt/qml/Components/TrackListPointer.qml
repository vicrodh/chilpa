import QtQuick
import com.blitzfc.chilpa
import "../Theme"

// The one pointer of a track list: hover and clicks are resolved here, once
// for the whole list, so a row carries no mouse handling of its own.
MouseArea {
    id: pointer

    property Item host
    property ListView rows
    property TrackColumns cols
    property TrackMenu menu
    property CreditLine measure
    cursorShape: Qt.PointingHandCursor

    property int pressedRow: -1
    property real pressedY: 0
    property bool dragging: false
    property bool pressedRating: false
    // What is being dragged, for the ghost that follows the pointer.
    property string dragTitle
    property string dragDetail
    property int dragCount: 1
    property bool dropped: false

    hoverEnabled: true
    acceptedButtons: Qt.LeftButton | Qt.RightButton

    function rowAt(y) {
        return rows.indexAt(Metrics.gap2, y + rows.contentY)
    }
    function track() {
        rows.hovered = containsMouse ? rowAt(mouseY) : -1
        rows.artistHovered = containsMouse && onArtist(rows.hovered, mouseX, mouseY) ? rows.hovered : -1
        rows.ratingHovered = ratingAt(rows.hovered, mouseX, mouseY)
        rows.actionHovered = containsMouse ? actionAt(rows.hovered, mouseX, mouseY) : 0
        rows.moveHovered = containsMouse ? moveAt(rows.hovered, mouseX, mouseY) : 0
    }
    // The arrow at that point of a row kept in its own order: before the
    // number, one over the other in a rich row and side by side in a
    // compact one.
    function moveAt(row, x, y) {
        const item = cols.reorder && row >= 0 ? rows.itemAtIndex(row) : null
        const left = item ? x - item.x - cols.moveX : -1
        if (!item || item.header || !item.available || left < 0 || left >= cols.move)
            return 0
        if (!cols.rich)
            return left < Metrics.moveCellCompact ? 1 : 2
        const cell = Math.floor((y + rows.contentY - item.y - item.height / 2) / Metrics.moveCell)
        return cell === -1 ? 1 : cell === 0 ? 2 : 0
    }
    function move(row, x, y) {
        const arrow = moveAt(row, x, y)
        if (arrow > 0)
            host.model.moveInPlaylist(row, arrow === 1 ? -1 : 1)
        return arrow > 0
    }
    function actionAt(row, x, y) {
        const item = row >= 0 ? rows.itemAtIndex(row) : null
        return item ? item.end.actionAt(x - item.x - item.end.x, y + rows.contentY - item.y) : 0
    }
    function ratingAt(row, x, y) {
        const item = row >= 0 ? rows.itemAtIndex(row) : null
        return item ? item.ratingAt(x - item.x, y + rows.contentY - item.y) : 0
    }
    // Whether the pointer is on the row's artist: its own column in
    // compact rows, the line under the title in rich ones. Worked out
    // here, once for the list, and not by a handler on every row.
    function onArtist(row, x, y) {
        const item = row >= 0 ? rows.itemAtIndex(row) : null
        if (!item || item.header || item.artist === "" || !host.model.canGo(row, "artist"))
            return false
        return item.containsArtist(x - item.x, y + rows.contentY - item.y)
    }

    onPressed: (mouse) => {
        dropped = false
        pressedRating = ratingAt(rowAt(mouse.y), mouse.x, mouse.y) > 0
        const pressed = rows.itemAtIndex(rowAt(mouse.y))
        const onAction = actionAt(rowAt(mouse.y), mouse.x, mouse.y) > 0 || moveAt(rowAt(mouse.y), mouse.x, mouse.y) > 0
        if (mouse.button === Qt.LeftButton && host.reorderable && !pressedRating && !onAction && pressed && pressed.available) {
            pressedRow = rowAt(mouse.y)
            pressedY = mouse.y
            dragging = false
            dragCount = pressed.selected && host.model.selectedCount > 1 ? host.model.selectedCount : 1
            dragTitle = pressed.title
            dragDetail = [pressed.artist, pressed.album].filter((part) => part !== "").join(" · ")
        }
    }
    onPositionChanged: (mouse) => {
        track()
        if (pressedRow >= 0 && Math.abs(mouse.y - pressedY) > Qt.styleHints.startDragDistance)
            dragging = true
    }
    onReleased: (mouse) => {
        // The release that ends a drag is a drop, not also a click.
        dropped = dragging
        if (dragging && pressedRow >= 0) {
            const target = rowAt(mouse.y)
            if (target >= 0)
                host.model.moveInPlaylistTo(pressedRow, target)
        }
        pressedRow = -1
        dragging = false
    }
    onContainsMouseChanged: track()
    onClicked: (mouse) => {
        const row = rowAt(mouse.y)
        if (row < 0 || dropped)
            return
        if (mouse.button === Qt.RightButton) {
            host.model.select(row, 4)
            menu.show(row, mouse.x, rows.y + mouse.y)
            return
        }
        if (move(row, mouse.x, mouse.y))
            return
        const action = actionAt(row, mouse.x, mouse.y)
        if (action === 2) {
            const at = rows.itemAtIndex(row).end.menuPoint(host)
            host.model.select(row, 4)
            menu.show(row, at.x, at.y)
        }
        if (action > 0)
            return rows.itemAtIndex(row).end.act()
        const star = ratingAt(row, mouse.x, mouse.y)
        if (pressedRating && star === 0)
            return
        if (star > 0) {
            const item = rows.itemAtIndex(row)
            Ratings.rate("track", item.trackId, item.rating === star ? 0 : star)
            return
        }
        const toggle = host.selectionMode || (mouse.modifiers & Qt.ControlModifier) || (mouse.modifiers & Qt.MetaModifier)
        if (!toggle && !(mouse.modifiers & Qt.ShiftModifier) && onArtist(row, mouse.x, mouse.y)) {
            const label = rows.itemAtIndex(row).artistLabel
            measure.font = label.font
            measure.credit = host.model.creditOf(row)
            host.model.go(row, "artist:" + measure.placeAt(label.mapFromItem(pointer, mouse.x, mouse.y).x))
            return
        }
        host.model.select(row, (toggle ? 1 : 0) | (mouse.modifiers & Qt.ShiftModifier ? 2 : 0))
    }
    onDoubleClicked: (mouse) => {
        const row = rowAt(mouse.y)
        // A second quick press on an arrow is one more step, not a play.
        if (row >= 0 && mouse.button === Qt.LeftButton && move(row, mouse.x, mouse.y))
            return
        if (row >= 0 && mouse.button === Qt.LeftButton && !pressedRating && !ratingAt(row, mouse.x, mouse.y) && !actionAt(row, mouse.x, mouse.y))
            host.model.play(row)
    }
}
