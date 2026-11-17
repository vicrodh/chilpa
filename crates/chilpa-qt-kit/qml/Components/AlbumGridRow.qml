// hot-path: the shared grid recycles a bounded number of rows.
import QtQuick
import "../Theme"

// Row geometry shared by catalog models and bounded collection windows.
Item {
    id: row
    required property var owner
    required property int first
    required property int span
    required property int revision
    required property string group
    property Component cardDelegate
    property Component listDelegate
    property Component quickDelegate
    readonly property real lead: group !== "" ? Metrics.rowGroup : 0
    readonly property bool onScreen: owner.visible && y + height > owner.visibleTop && y < owner.visibleTop + (owner.viewport ? owner.viewport.height : owner.scrollView.height) && (!owner.shelf || x + width > owner.scrollView.contentX && x < owner.scrollView.contentX + owner.scrollView.width)
    // Keep existing cards across a hidden tab without populating new rows there.
    property bool nearby: false
    property int cardRevision: 0
    readonly property bool inBand: !owner.viewport || y + height >= owner.visibleTop - owner.viewport.height && y < owner.visibleTop + owner.viewport.height * 2
    function updateCards() {
        if (!owner?.visible) return
        nearby = inBand
        cardRevision = revision
    }
    onInBandChanged: updateCards()
    onRevisionChanged: updateCards()
    onVisibleChanged: updateCards()
    Component.onCompleted: updateCards()
    width: owner.shelf ? owner.cardWidth : owner.technical ? owner.scrollView.contentWidth : owner.width
    height: lead + owner.rowHeight + (quick.expanded ? Metrics.gap8 + quick.panelHeight : 0)
    AlbumGroupHeading {
        visible: row.group !== ""
        x: Metrics.cardRing
        width: row.owner.layoutWidth
        text: row.group
    }
    Loader {
        active: row.owner.list && row.nearby
        x: Metrics.cardRing
        y: row.lead
        width: row.owner.layoutWidth
        sourceComponent: row.listDelegate
    }
    Repeater {
        model: row.owner.list || !row.nearby ? 0 : row.owner.shelf ? 1 : row.owner.columns
        delegate: row.cardDelegate
    }
    Loader {
        id: quick
        objectName: "albumQuickLoader"
        y: row.lead + row.owner.rowHeight + Metrics.gap8
        x: (row.owner.shelf ? row.owner.scrollView.contentX - row.x : row.owner.viewportX) + Metrics.cardRing
        width: Math.max(0, row.owner.viewportWidth - Metrics.cardRing * 2) // derived: inset for both selection rings
        readonly property int expandedIndex: row.owner.model ? row.owner.model.expanded : -1
        readonly property bool expanded: expandedIndex >= row.first && expandedIndex < row.first + row.span
        readonly property bool requested: expanded && row.owner.visible
        property var suspended: null
        readonly property real panelHeight: suspended && (!item || item.albumId === suspended.album && !item.populated) ? suspended.height : height
        // Page retirement severs this binding after cancelling creation.
        property bool loadRequested: false
        active: loadRequested
        onExpandedIndexChanged: suspended = null
        onRequestedChanged: {
            if (!requested && expanded && item)
                suspended = { album: item.albumId, version: item.selectedVersion || item.version, height: panelHeight }
            loadRequested = requested
        }
        sourceComponent: row.quickDelegate
    }
}
