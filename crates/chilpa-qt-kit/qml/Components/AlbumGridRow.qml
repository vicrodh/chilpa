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
    readonly property bool nearby: !owner.viewport || y + height >= owner.visibleTop - owner.viewport.height && y < owner.visibleTop + owner.viewport.height * 2
    width: owner.shelf ? owner.cardWidth : owner.technical ? owner.scrollView.contentWidth : owner.width
    height: lead + owner.rowHeight + (quick.active ? Metrics.gap8 + quick.height : 0)
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
        active: !!row.owner.model && row.owner.model.expanded >= row.first && row.owner.model.expanded < row.first + row.span
        sourceComponent: row.quickDelegate
    }
}
