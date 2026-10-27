import QtQuick
import "../Theme"

// I keep a row's geometry while its visual contents are outside the page.
Item {
    id: slot
    property bool inViewport: true
    property bool layoutReady: false
    // I wait for the view to place the slot before I build its visible contents.
    function finishLayout() { layoutReady = true }
    Component.onCompleted: Qt.callLater(finishLayout)
    property bool immediate: true
    property bool header: false
    property bool canRate: false
    property var credits: ({})
    property Component content
    readonly property int contentBuilds: body.builds
    readonly property Item sourceBadge: body.item ? body.item.sourceBadge : null
    readonly property Item end: body.item ? body.item.end : null
    readonly property Item creditedTitle: body.item ? body.item.creditedTitle : null
    readonly property Text artistLabel: body.item ? body.item.artistLabel : null
    readonly property Text albumText: body.item ? body.item.albumText : null
    readonly property Item headerComposerLabel: body.item ? body.item.headerComposerLabel : null
    readonly property Item groupActionArea: body.item ? body.item.groupActionArea : null
    function groupActionAt(from, x, y) { return body.item ? body.item.groupActionAt(from, x, y) : 0 }
    function creditAt(from, x, y) {
        if (header || !credits.links) return null
        let link = creditedTitle ? creditedTitle.linkAt(from, x, y) : ""
        const label = artistLabel
        if (!link && label && label.visible) {
            const point = label.mapFromItem(from, x, y)
            link = label.linkAt(point.x, point.y)
        }
        return link !== "" ? credits.links[Number(link)] || null : null
    }
    function containsLabel(from, label, x, y) {
        if (!label || !label.visible) return false
        const point = label.mapFromItem(from, x, y)
        return point.x >= 0 && point.y >= 0 && point.x < Math.min(label.width, label.contentWidth) && point.y < label.height
    }
    function containsArtist(x, y) {
        const label = artistLabel
        if (header || !label || !label.visible) return false
        const point = label.mapFromItem(slot, x, y)
        return point.x >= 0 && point.y >= 0 && point.x < Math.min(label.width, label.contentWidth) && point.y < label.height
            && (!credits.links || label.linkAt(point.x, point.y) !== "")
    }
    function ratingAt(x, y) {
        const stars = body.item ? body.item.ratingStars : null
        if (header || !canRate || !stars || !stars.visible) return 0
        const point = stars.mapFromItem(slot, x, y)
        return point.x >= 0 && point.x < stars.width && point.y >= 0 && point.y < stars.height
            ? Math.floor(point.x / stars.width * Metrics.ratingMaximum) + 1 : 0
    }
    Loader { id: body; property int builds: 0; onLoaded: builds++; anchors.fill: parent; active: slot.layoutReady && slot.inViewport; asynchronous: !slot.immediate; sourceComponent: slot.content }
}
