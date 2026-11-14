import QtQuick
import QtQuick.Controls.Basic
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

ListView {
    id: list
    objectName: "batchPreview"
    property var batch
    property string hardlinkPolicy: ""
    property var grouped: []
    signal groupOpened(var group)
    signal lookupChosen(var group)
    signal groupingChosen(var group)
    model: batch ? batch.count : 0
    clip: true; reuseItems: true; spacing: Metrics.gap8
    cacheBuffer: Metrics.rowRich * 2 // derived: two rows of runway
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}
    function seek() {
        if (!visible || !batch || count === 0) return
        const visibleRows = Array.from(contentItem.children).filter(row => row.index !== undefined && row.y + row.height > contentY && row.y < contentY + height)
        if (visibleRows.length) batch.seek(Math.min(...visibleRows.map(row => row.index)), Math.max(...visibleRows.map(row => row.index)))
        else batch.seek(0, 99)
    }
    onContentYChanged: seek()
    onCountChanged: Qt.callLater(seek)
    onHeightChanged: Qt.callLater(seek)
    onVisibleChanged: if (visible) Qt.callLater(seek)
    delegate: Loader {
        id:cell
        required property int index
        width: list.width
        height:item ? item.implicitHeight : Metrics.rowRich
        readonly property var entry: { try { return JSON.parse(list.batch.entry(index,list.batch.revision) || "{}") } catch (_) { return {} } }
        sourceComponent:list.batch.kind === "groups" ? groupRow : fileRow
        onHeightChanged: Qt.callLater(list.seek)
        Component { id:fileRow; BatchPreviewRow {
            entry:cell.entry; busy:list.batch.busy; hardlinkPolicy:list.hardlinkPolicy;selectable:list.batch.kind !== "plan"
            onSelectionChosen:selected=>list.batch.select(JSON.stringify({mode:"keys",keys:[entry.key],selected:selected}))
        } }
        Component { id:groupRow; BatchGroupRow {
            entry:cell.entry; busy:list.batch.busy; included:list.grouped.some(group=>group.key === entry.key)
            onSelectionChosen:selected=>list.batch.select(JSON.stringify({mode:"groups",groups:[entry.key],selected:selected}))
            onOpened:list.groupOpened(entry)
            onLookupChosen:list.lookupChosen(entry)
            onGroupingChosen:list.groupingChosen(entry)
        } }
    }
}
