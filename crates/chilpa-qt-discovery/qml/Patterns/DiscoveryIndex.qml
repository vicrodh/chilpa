import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

Item {
    id: index
    property var entries: []
    property int total: 0
    property int page: 0
    property bool loading: false
    property bool ready: true
    readonly property alias scrollView: grid
    signal activated(var entry)
    signal pageRequested(int page)
    function detail(entry) {
        if (entry.kind === "genre") return I18n.trCount("{} style in your library", "{} styles in your library", entry.styles || 0, I18n.rev)
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
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: paging.top
        anchors.bottomMargin: Metrics.gap12
        readonly property int columns: Math.max(1, Math.floor((width + Metrics.gap12) / Metrics.discoveryEntryWidth))
        cellWidth: width / columns
        cellHeight: Metrics.discoveryEntryHeight + Metrics.gap12
        model: index.entries
        clip: true
        reuseItems: true
        cacheBuffer: cellHeight
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar {}
        delegate: Item {
            required property var modelData
            width: grid.cellWidth; height: grid.cellHeight
            DiscoveryEntry {
                width: parent.width - Metrics.gap12; height: Metrics.discoveryEntryHeight
                entry: modelData; detail: index.detail(modelData)
                onClicked: index.activated(modelData)
            }
        }
    }
    Row {
        id: paging
        anchors.bottom: parent.bottom; anchors.horizontalCenter: parent.horizontalCenter
        spacing: Metrics.gap12
        visible: index.total > Metrics.discoveryPageSize
        height: visible ? implicitHeight : 0
        PillButton { text: I18n.tr("Previous", I18n.rev); enabled: index.page > 0 && !index.loading; onClicked: index.pageRequested(index.page - 1) }
        Text { anchors.verticalCenter: parent.verticalCenter; text: (index.page + 1) + " / " + Math.ceil(index.total / Metrics.discoveryPageSize); font.pointSize: Type.meta; color: Theme.color.textSecondary }
        PillButton { text: I18n.tr("Next", I18n.rev); enabled: (index.page + 1) * Metrics.discoveryPageSize < index.total && !index.loading; onClicked: index.pageRequested(index.page + 1) }
    }
}
