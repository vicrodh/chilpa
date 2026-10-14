import QtQuick
import "../Theme"

// I keep a row's geometry while its visual contents are outside the page.
Item {
    id: slot
    property bool inViewport: true
    property bool immediate: true
    property bool header: false
    property bool canRate: false
    property Component content
    readonly property Item sourceBadge: body.item ? body.item.sourceBadge : null
    readonly property Item end: body.item ? body.item.end : null
    readonly property Text artistLabel: body.item ? body.item.artistLabel : null
    readonly property Text albumText: body.item ? body.item.albumText : null
    readonly property Item headerComposerLabel: body.item ? body.item.headerComposerLabel : null
    readonly property Item groupActionArea: body.item ? body.item.groupActionArea : null
    function groupActionAt(from, x, y) { return body.item ? body.item.groupActionAt(from, x, y) : 0 }
    function containsArtist(x, y) {
        const label = artistLabel
        if (header || !label || !label.visible) return false
        const point = label.mapFromItem(slot, x, y)
        return point.x >= 0 && point.y >= 0 && point.x < Math.min(label.width, label.contentWidth) && point.y < label.height
    }
    function ratingAt(x, y) {
        const stars = body.item ? body.item.ratingStars : null
        if (header || !canRate || !stars || !stars.visible) return 0
        const point = stars.mapFromItem(slot, x, y)
        return point.x >= 0 && point.x < stars.width && point.y >= 0 && point.y < stars.height
            ? Math.floor(point.x / stars.width * Metrics.ratingMaximum) + 1 : 0
    }
    Loader { id: body; anchors.fill: parent; active: slot.inViewport; asynchronous: !slot.immediate; sourceComponent: slot.content }
}
