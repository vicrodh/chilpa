import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

Item {
    id: parentView
    property var entries: []
    property var dataSource: null
    readonly property int total: dataSource ? dataSource.count : entries.length
    property bool ready: true
    property string kind: "genres"
    readonly property bool portraits: kind === "composers" || kind === "genres"
    readonly property int entryWidth: portraits ? Metrics.sceneCardWidth : Metrics.discoveryEntryWidth
    readonly property int entryHeight: portraits ? Metrics.sceneCardHeight : Metrics.discoveryEntryHeight
    readonly property alias scrollView: grid
    signal activated(var entry)
    function decoded(row) { try { return JSON.parse(row || "{}") } catch (_) { return {} } }
    function requestVisible() {
        if (!visible || !dataSource || total === 0) return
        const first = Math.max(0, Math.floor(grid.contentY / grid.cellHeight) * grid.columns)
        dataSource.seek(first, Math.ceil((grid.contentY + grid.height) / grid.cellHeight) * grid.columns + grid.columns - 1)
    }
    onTotalChanged: Qt.callLater(requestVisible)
    onVisibleChanged: if (visible) Qt.callLater(requestVisible)
    function detail(entry) {
        if (entry.kind === "genre") return [I18n.trCount("{} album", "{} albums", entry.albums || 0, I18n.rev),
            (entry.looseTracks || 0) > 0 ? I18n.trCount("{} albumless track", "{} albumless tracks", entry.looseTracks, I18n.rev) : ""].filter(Boolean).join(" · ")
        if (entry.kind === "recording") {
            const seconds = Math.floor((entry.duration || 0) / 1000)
            const duration = Math.floor(seconds / 60) + ":" + String(seconds % 60).padStart(2, "0")
            return [entry.artist || "", entry.source || "", I18n.trCount("{} part", "{} parts", entry.tracks || 0, I18n.rev), seconds > 0 ? duration : ""].filter(Boolean).join(" · ")
        }
        if (entry.kind === "work") return (entry.composers || []).join(" · ") || I18n.tr("Composer not tagged", I18n.rev)
        return I18n.trCount("{} album", "{} albums", entry.albums || 0, I18n.rev)
    }
    GridView {
        id: grid
        objectName: "discoveryEntries"
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap12
        readonly property int columns: Math.max(1, Math.floor((width + Metrics.gap12) / parentView.entryWidth))
        cellWidth: width / columns
        cellHeight: parentView.entryHeight + Metrics.gap12
        model: parentView.total
        clip: true
        reuseItems: true
        cacheBuffer: cellHeight
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar {}
        onContentYChanged: parentView.requestVisible()
        onHeightChanged: Qt.callLater(parentView.requestVisible)
        onColumnsChanged: Qt.callLater(parentView.requestVisible)
        delegate: Item {
            id: cell
            required property int index
            readonly property var modelData: parentView.dataSource ? parentView.decoded(parentView.dataSource.entry(index, parentView.dataSource.revision)) : parentView.entries[index] || ({})
            width: grid.cellWidth; height: grid.cellHeight
            DiscoveryEntry {
                objectName: "discoveryEntry:" + (cell.modelData.name || "")
                width: parent.width - Metrics.gap12; height: parentView.entryHeight
                activeImages: grid.visible && cell.y + cell.height > grid.contentY && cell.y < grid.contentY + grid.height
                entry: modelData; detail: parentView.detail(modelData)
                enabled: !!modelData.kind
                onClicked: parentView.activated(modelData)
            }
        }
    }
}
