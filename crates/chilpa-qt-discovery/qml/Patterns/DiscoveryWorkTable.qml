import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    id: table
    property var entries: []
    property var dataSource: null
    property bool recordings: false
    // The page whose margin holds the scrollbar: hover reaches it there.
    property Item scrollbarOverlay: null
    readonly property int total: dataSource ? dataSource.count : entries.length
    property bool ready: true
    readonly property alias scrollView: rows
    readonly property bool counts: width >= Metrics.discoveryTableCountsMin
    readonly property real countWidth: counts ? Metrics.discoveryTableCountWidth + Metrics.gap12 : 0
    readonly property real inset: Metrics.gap12
    readonly property real coverSize: Metrics.rowRich - Metrics.gap12
    readonly property real arrowWidth: Metrics.iconSm + inset * 2 // derived: icon and two insets
    readonly property real textWidth: Math.max(0, width - arrowWidth - countWidth * 2) // derived: space for two count columns
    readonly property real nameWidth: counts ? textWidth * Metrics.discoveryTableTitleRatio : textWidth
    readonly property real creditWidth: counts ? textWidth - nameWidth : 0
    signal activated(var entry)
    function decoded(row) { try { return JSON.parse(row || "{}") } catch (_) { return {} } }
    function duration(ms) {
        const seconds = Math.floor((ms || 0) / 1000)
        if (seconds === 0) return ""
        const minutes = Math.floor(seconds / 60)
        const tail = String(seconds % 60).padStart(2, "0")
        return minutes < 60 ? minutes + ":" + tail
                          : Math.floor(minutes / 60) + ":" + String(minutes % 60).padStart(2, "0") + ":" + tail
    }
    function requestVisible() {
        if (!visible || !dataSource || total === 0) return
        dataSource.seek(Math.max(0, Math.floor(rows.contentY / Metrics.rowRich)), Math.ceil((rows.contentY + rows.height) / Metrics.rowRich) + 1)
    }
    onTotalChanged: Qt.callLater(requestVisible)
    onVisibleChanged: if (visible) Qt.callLater(requestVisible)

    Row {
        id: header
        width: parent.width; height: Metrics.rowCompact
        Repeater {
            model: table.recordings
                ? [I18n.tr("Recording", I18n.rev), I18n.tr("Performer", I18n.rev), I18n.tr("Parts", I18n.rev), I18n.tr("Duration", I18n.rev)]
                : [I18n.tr("Composition", I18n.rev), I18n.tr("Composer", I18n.rev), I18n.tr("Recordings", I18n.rev), I18n.tr("Tracks", I18n.rev)]
            Text {
                required property string modelData
                required property int index
                width: index === 0 ? table.nameWidth : index === 1 ? table.creditWidth : table.countWidth
                visible: index === 0 || table.counts
                height: header.height; verticalAlignment: Text.AlignVCenter
                leftPadding: index === 0 ? table.inset * 2 + table.coverSize : index === 1 ? table.inset : Metrics.gap4 // derived: align after the cover and both insets
                rightPadding: index <= 1 ? table.inset : Metrics.gap4
                horizontalAlignment: index < 2 ? Text.AlignLeft : Text.AlignHCenter
                text: modelData; elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.meta; font.weight: Type.semibold
            }
        }
    }
    ListView {
        id: rows
        objectName: "discoveryEntries"
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: header.bottom; anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap12
        model: table.total; clip: true; reuseItems: true
        cacheBuffer: Metrics.rowRich * 2
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar { objectName: "discoveryScrollbar"; gutter: Metrics.pageGutter; overlayParent: table.scrollbarOverlay }
        onContentYChanged: table.requestVisible()
        onHeightChanged: Qt.callLater(table.requestVisible)
        delegate: Button {
            id: row
            required property int index
            readonly property var modelData: table.dataSource ? table.decoded(table.dataSource.entry(index, table.dataSource.revision)) : table.entries[index] || ({})
            readonly property string credit: table.recordings ? modelData.artist || ""
                : (modelData.composers || []).join(" · ") || I18n.tr("Composer not tagged", I18n.rev)
            readonly property string length: table.duration(modelData.duration)
            readonly property string sources: table.recordings ? modelData.source || "" : (modelData.sources || []).join(", ")
            readonly property string quantity: table.recordings
                ? I18n.trCount("{} part", "{} parts", modelData.tracks || 0, I18n.rev)
                : I18n.trCount("{} recording", "{} recordings", modelData.recordings || 0, I18n.rev)
            readonly property string releaseYear: table.recordings && modelData.year > 0 ? String(modelData.year) : ""
            readonly property bool inViewport: table.visible && row.y + row.height > rows.contentY && row.y < rows.contentY + rows.height
            objectName: "discoveryEntry:" + (modelData.name || "")
            enabled: !!modelData.kind
            width: rows.width; height: Metrics.rowRich; padding: 0
            hoverEnabled: true
            Accessible.name: modelData.name || ""
            Accessible.description: [credit, releaseYear, quantity, length, sources,
                table.recordings ? "" : I18n.trCount("{} track", "{} tracks", modelData.tracks || 0, I18n.rev)].filter(Boolean).join(" · ")
            onClicked: table.activated(modelData)
            Keys.onReturnPressed: clicked()
            Keys.onEnterPressed: clicked()
            background: Rectangle {
                objectName: "discoveryWorkStripe"
                radius: Metrics.radiusRow
                color: row.down ? Theme.color.stateSelected : row.hovered ? Theme.color.stateHover
                    : row.index % 2 ? Theme.color.surfacePanel : "transparent"
                FocusRing { visible: row.activeFocus }
            }
            contentItem: Row {
                Item {
                    width: table.nameWidth
                    height: row.height
                    ArtTile {
                        objectName: "discoveryWorkArt"
                        x: table.inset
                        width: table.coverSize; height: width
                        anchors.verticalCenter: parent.verticalCenter
                        radius: Metrics.radiusRow
                        source: row.inViewport ? row.modelData.artwork || "" : ""
                        fallbackGlyph: table.recordings ? Icons.disc : Icons.music
                    }
                    Column {
                        x: table.inset * 2 + table.coverSize
                        width: Math.max(0, parent.width - x - table.inset)
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: Metrics.gap4
                        Text {
                            objectName: "discoveryWorkTitle"
                            width: parent.width
                            text: row.modelData.name || ""; textFormat: Text.PlainText; elide: Text.ElideRight
                            color: Theme.color.textPrimary; font.pointSize: Type.body; font.weight: Type.semibold
                        }
                        Text {
                            objectName: "discoveryWorkDetails"
                            width: parent.width
                            visible: text !== ""
                            text: [table.counts ? "" : row.credit, row.releaseYear, row.sources,
                                !table.counts || !table.recordings ? row.length : "",
                                table.counts ? "" : row.quantity].filter(Boolean).join(" · ")
                            textFormat: Text.PlainText; elide: Text.ElideRight
                            color: Theme.color.textSecondary; font.pointSize: Type.meta
                        }
                    }
                }
                Repeater {
                    model: [row.credit, String((table.recordings ? row.modelData.tracks : row.modelData.recordings) || 0),
                        table.recordings ? row.length : String(row.modelData.tracks || 0)]
                    Text {
                        required property string modelData
                        required property int index
                        width: index === 0 ? table.creditWidth : table.countWidth
                        visible: table.counts
                        height: row.height; verticalAlignment: Text.AlignVCenter
                        leftPadding: table.inset; rightPadding: table.inset
                        horizontalAlignment: index === 0 ? Text.AlignLeft : Text.AlignHCenter
                        text: modelData; textFormat: Text.PlainText; elide: Text.ElideRight
                        color: Theme.color.textSecondary; font.pointSize: Type.body
                    }
                }
                Item {
                    width: table.arrowWidth; height: row.height
                    Icon { anchors.centerIn: parent; glyph: Icons.chevronRight; size: Metrics.iconSm; color: Theme.color.textTertiary }
                }
            }
            ToolTip.visible: hovered && (modelData.name || "").length > 0
            ToolTip.text: (modelData.name || "") + "\n" + Accessible.description
        }
    }
}
