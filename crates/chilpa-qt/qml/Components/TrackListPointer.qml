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
        rows.moveHovered = containsMouse ? moveAt(rows.hovered, mouseX) : 0
    }
    // The arrow at that point of a row kept in its own order: they sit
    // where the track's number would be.
    function moveAt(row, x) {
        const item = cols.reorder && row >= 0 ? rows.itemAtIndex(row) : null
        if (!item || item.header || !item.available)
            return 0
        const cell = Math.floor((x - item.x - cols.indexX) / Metrics.rowAction)
        return cell === 0 ? 1 : cell === 1 ? 2 : 0
    }
    function move(row, x) {
        const arrow = moveAt(row, x)
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
        pressedRating = ratingAt(rowAt(mouse.y), mouse.x, mouse.y) > 0
        const pressed = rows.itemAtIndex(rowAt(mouse.y))
        const onAction = actionAt(rowAt(mouse.y), mouse.x, mouse.y) > 0 || moveAt(rowAt(mouse.y), mouse.x) > 0
        if (mouse.button === Qt.LeftButton && host.reorderable && !pressedRating && !onAction && pressed && pressed.available) {
            pressedRow = rowAt(mouse.y)
            pressedY = mouse.y
            dragging = false
        }
    }
    onPositionChanged: (mouse) => {
        track()
        if (pressedRow >= 0 && Math.abs(mouse.y - pressedY) > Qt.styleHints.startDragDistance)
            dragging = true
    }
    onReleased: (mouse) => {
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
        if (row < 0)
            return
        if (mouse.button === Qt.RightButton) {
            host.model.select(row, 4)
            menu.show(row, mouse.x, rows.y + mouse.y)
            return
        }
        if (move(row, mouse.x))
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
        if (row >= 0 && mouse.button === Qt.LeftButton && move(row, mouse.x))
            return
        if (row >= 0 && mouse.button === Qt.LeftButton && !pressedRating && !ratingAt(row, mouse.x, mouse.y) && !actionAt(row, mouse.x, mouse.y))
            host.model.play(row)
    }
}
