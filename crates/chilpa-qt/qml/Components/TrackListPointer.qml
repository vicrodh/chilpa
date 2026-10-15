import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

    // Hover and clicks are resolved once for the whole list.
MouseArea {
    id: pointer

    property Item host
    property ListView rows
    property TrackColumns cols
    property TrackMenu menu
    property QueueMenu groupMenu
    property CreditLine measure
    cursorShape: Qt.PointingHandCursor

    property Item sourceTip: null
    onVisibleChanged: if (!visible && sourceTip) { TipState.hide(sourceTip); sourceTip = null }
    Component.onDestruction: if (sourceTip) TipState.hide(sourceTip)

    property int pressedRow: -1
    property real pressedY: 0
    property bool dragging: false
    property bool pressedRating: false
    // What is being dragged, for the ghost that follows the pointer.
    property string dragTitle
    property string dragDetail
    property int dragCount: 1
    property bool dropped: false
    // Where what is dragged would land: on that row, after it when it comes
    // from above and before it when it comes from below.
    readonly property int dropRow: dragging && containsMouse ? rowAt(mouseY) : -1
    readonly property bool dropAfter: dropRow > pressedRow

    hoverEnabled: true
    onWheel: (wheel) => { host.userScrollStarted(); wheel.accepted = false }
    // A movable row owns its press so the page cannot steal it for scrolling.
    preventStealing: pressedRow >= 0
    acceptedButtons: Qt.LeftButton | Qt.RightButton

    function rowAt(y) {
        return rows.indexAt(Metrics.gap2, y + rows.contentY)
    }
    function track() {
        rows.hovered = containsMouse ? rowAt(mouseY) : -1
        rows.artistHovered = containsMouse && onArtist(rows.hovered, mouseX, mouseY) ? rows.hovered : -1
        rows.albumHovered = containsMouse && onAlbum(rows.hovered, mouseX, mouseY) ? rows.hovered : -1
        const item = rows.hovered >= 0 ? rows.itemAtIndex(rows.hovered) : null
        const badge = item && !item.header && cols.sourceWidth > 0 && mouseX >= cols.sourceX && mouseX < cols.sourceX + cols.sourceWidth ? item.sourceBadge : null
        if (sourceTip !== badge) {
            if (sourceTip) TipState.hide(sourceTip)
            sourceTip = badge
            if (badge) TipState.show(item.sourceName, badge)
        }
        rows.ratingHovered = ratingAt(rows.hovered, mouseX, mouseY)
        rows.actionHovered = containsMouse ? (groupActionAt(rows.hovered, mouseX, mouseY) || actionAt(rows.hovered, mouseX, mouseY)) : 0
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
        return item && item.end ? item.end.actionAt(x - item.x - item.end.x, y + rows.contentY - item.y) : 0
    }
    function groupActionAt(row, x, y) {
        const item = row >= 0 ? rows.itemAtIndex(row) : null
        return item ? item.groupActionAt(pointer, x, y) : 0
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

    function onAlbum(row, x, y) {
        const item = row >= 0 ? rows.itemAtIndex(row) : null
        if (!item || item.header || item.album === "" || !host.model.canGo(row, "album")) return false
        return insideText(item, item.albumText, x, y)
    }
    function insideText(item, label, x, y) {
        if (!label || !label.visible) return false
        const point = label.mapFromItem(item, x - item.x, y + rows.contentY - item.y)
        return point.x >= 0 && point.y >= 0 && point.x < Math.min(label.width, label.contentWidth) && point.y < label.height
    }
    function onHeaderComposer(item, x, y) {
        return item && item.headerWork && item.headerComposerTarget !== ""
            && insideText(item, item.headerComposerLabel, x, y)
    }

    onPressed: (mouse) => {
        if (mouse.button === Qt.LeftButton && host.reorderable) host.userScrollStarted()
        dropped = false
        pressedRating = ratingAt(rowAt(mouse.y), mouse.x, mouse.y) > 0
        const pressed = rows.itemAtIndex(rowAt(mouse.y))
        const onAction = actionAt(rowAt(mouse.y), mouse.x, mouse.y) > 0 || moveAt(rowAt(mouse.y), mouse.x, mouse.y) > 0
        if (mouse.button === Qt.LeftButton && host.reorderable && !pressedRating && !onAction && pressed && !pressed.header && pressed.available) {
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
        const header = rows.itemAtIndex(row)
        const groupAction = groupActionAt(row, mouse.x, mouse.y)
        if (mouse.button === Qt.LeftButton && groupAction > 0) {
            if (groupAction === 3) {
                const at = header.groupActionArea.mapToItem(host, 0, header.groupActionArea.height + Metrics.gap4)
                groupMenu.showGroup(row, header.headerLabel, host.width - Metrics.menuWidth - Metrics.gap10, at.y)
            } else host.model.groupAction(row, groupAction === 1 ? "play" : "shuffle", header.headerLabel)
            return
        }
        if (mouse.button === Qt.LeftButton && onHeaderComposer(header, mouse.x, mouse.y)) {
            const target = header.headerComposerTarget
            if (target.startsWith("artist:")) Navigation.openArtist(target.slice(7))
            else Navigation.goKey("source", target)
            return
        }
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
        if (!toggle && !(mouse.modifiers & Qt.ShiftModifier) && onAlbum(row, mouse.x, mouse.y)) {
            host.model.go(row, "album")
            return
        }
        host.cursorRow = row
        host.forceActiveFocus()
        host.model.select(row, (toggle ? 1 : 0) | (mouse.modifiers & Qt.ShiftModifier ? 2 : 0))
    }
    onDoubleClicked: (mouse) => {
        const row = rowAt(mouse.y)
        const header = row >= 0 ? rows.itemAtIndex(row) : null
        if (groupActionAt(row, mouse.x, mouse.y) || onHeaderComposer(header, mouse.x, mouse.y))
            return
        if (onAlbum(row, mouse.x, mouse.y) || onArtist(row, mouse.x, mouse.y))
            return
        // A second quick press on an arrow is one more step, not a play.
        if (row >= 0 && mouse.button === Qt.LeftButton && move(row, mouse.x, mouse.y))
            return
        if (row >= 0 && mouse.button === Qt.LeftButton && !pressedRating && !ratingAt(row, mouse.x, mouse.y) && !actionAt(row, mouse.x, mouse.y))
            host.model.play(row)
    }
}
