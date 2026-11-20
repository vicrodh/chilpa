import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Works as Roon lays them out: the work, its composers as links, then its
// date, period, form and instrumentation, and how many recordings the
// library holds. Date and instrumentation take a column only when some work
// of the result has one. Recordings of one work keep their own columns.
Item {
    id: table
    property var entries: []
    property var dataSource: null
    property bool recordings: false
    // {date, instrumentation}: whether the result has any value for them.
    property var columns: ({})
    // The page whose margin holds the scrollbar: hover reaches it there.
    property Item scrollbarOverlay: null
    readonly property int total: dataSource ? dataSource.count : entries.length
    property bool ready: true
    readonly property alias scrollView: rows
    readonly property bool counts: width >= Metrics.discoveryTableCountsMin
    readonly property real countWidth: counts ? Metrics.discoveryTableCountWidth + Metrics.gap12 : 0
    readonly property real factWidth: Metrics.discoveryTableCountWidth * 1.5 + Metrics.gap12 // derived: a word such as "Renaissance" beside a count column
    readonly property var factKeys: recordings ? [] : [columns.date ? "date" : "", "period", "form", columns.instrumentation ? "instrumentation" : ""].filter(Boolean)
    readonly property bool facts: counts && width >= Metrics.discoveryTableCountsMin + factKeys.length * factWidth
    readonly property int countColumns: recordings ? 2 : 1
    readonly property real inset: Metrics.gap12
    readonly property real coverSize: Metrics.rowRich - Metrics.gap12
    readonly property real arrowWidth: Metrics.iconSm + inset * 2 // derived: icon and two insets
    readonly property real textWidth: Math.max(0, width - arrowWidth - countWidth * countColumns - (facts ? factKeys.length * factWidth : 0))
    readonly property real nameWidth: counts ? textWidth * Metrics.discoveryTableTitleRatio : textWidth
    readonly property real creditWidth: counts ? textWidth - nameWidth : 0
    // The columns after the name.
    function cell(key, title, wide) { return {key:key, title:title, wide:wide, count:wide === countWidth} }
    readonly property var cells: !counts ? [] : recordings
        ? [cell("credit", I18n.tr("Performer", I18n.rev), creditWidth), cell("parts", I18n.tr("Parts", I18n.rev), countWidth), cell("length", I18n.tr("Duration", I18n.rev), countWidth)]
        : [cell("credit", I18n.tr("Composer", I18n.rev), creditWidth)].concat(facts ? factKeys.map(key => cell(key, factTitle(key), factWidth)) : [])
            .concat([cell("recordings", I18n.tr("Recordings", I18n.rev), countWidth)])
    signal activated(var entry)
    function factTitle(key) {
        return key === "date" ? I18n.tr("Date", I18n.rev) : key === "period" ? I18n.tr("Period", I18n.rev)
            : key === "form" ? I18n.tr("Form", I18n.rev) : I18n.tr("Instrumentation", I18n.rev)
    }
    function decoded(row) { try { return JSON.parse(row || "{}") } catch (_) { return {} } }
    function escaped(text) { return String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;") }
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
        Text {
            width: table.nameWidth; height: header.height; verticalAlignment: Text.AlignVCenter
            leftPadding: table.inset * 2 + table.coverSize; rightPadding: table.inset // derived: align after the cover and both insets
            text: table.recordings ? I18n.tr("Recording", I18n.rev) : I18n.tr("Composition", I18n.rev)
            elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.meta; font.weight: Type.semibold
        }
        Repeater {
            model: table.cells
            Text {
                required property var modelData
                required property int index
                width: modelData.wide; height: header.height; verticalAlignment: Text.AlignVCenter
                leftPadding: index === 0 ? table.inset : Metrics.gap4; rightPadding: index === 0 ? table.inset : Metrics.gap4
                horizontalAlignment: modelData.count ? Text.AlignHCenter : Text.AlignLeft
                text: modelData.title; elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.meta; font.weight: Type.semibold
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
            readonly property var composers: modelData.composers || []
            readonly property string credit: table.recordings ? modelData.artist || ""
                : composers.join(" · ") || I18n.tr("Composer not tagged", I18n.rev)
            readonly property string length: table.duration(modelData.duration)
            readonly property string sources: table.recordings ? modelData.source || "" : (modelData.sources || []).join(", ")
            readonly property string quantity: table.recordings
                ? I18n.trCount("{} part", "{} parts", modelData.tracks || 0, I18n.rev)
                : I18n.trCount("{} recording", "{} recordings", modelData.recordings || 0, I18n.rev)
            readonly property string releaseYear: table.recordings && modelData.year > 0 ? String(modelData.year) : ""
            // Facts not shown in a column of their own go to the details line.
            readonly property string folded: table.recordings || table.facts ? ""
                : ["date", "period", "form", "instrumentation"].map(key => (modelData[key] || {}).label || "").filter(Boolean).join(" · ")
            readonly property bool inViewport: table.visible && row.y + row.height > rows.contentY && row.y < rows.contentY + rows.height
            function value(key) {
                return key === "parts" ? String(modelData.tracks || 0) : key === "length" ? length
                    : key === "recordings" ? String(modelData.recordings || 0) : (modelData[key] || {}).label || ""
            }
            objectName: "discoveryEntry:" + (modelData.name || "")
            enabled: !!modelData.kind
            width: rows.width; height: Metrics.rowRich; padding: 0
            hoverEnabled: true
            Accessible.name: modelData.name || ""
            Accessible.description: [credit, releaseYear, folded, quantity, length, sources,
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
                            text: [table.counts ? "" : row.credit, row.releaseYear, row.folded, row.sources,
                                !table.counts || !table.recordings ? row.length : "",
                                table.counts ? "" : row.quantity].filter(Boolean).join(" · ")
                            textFormat: Text.PlainText; elide: Text.ElideRight
                            color: Theme.color.textSecondary; font.pointSize: Type.meta
                        }
                    }
                }
                Repeater {
                    model: table.cells
                    Text {
                        required property var modelData
                        required property int index
                        readonly property bool links: modelData.key === "credit" && !table.recordings && row.composers.length > 0
                        objectName: "discoveryWorkCell:" + modelData.key
                        width: modelData.wide
                        height: row.height; verticalAlignment: Text.AlignVCenter
                        leftPadding: table.inset; rightPadding: table.inset
                        horizontalAlignment: modelData.count ? Text.AlignHCenter : Text.AlignLeft
                        // Each composer opens its own page, as Roon's links do.
                        text: links ? row.composers.map((name, at) => "<a href=\"" + at + "\">" + table.escaped(name) + "</a>").join(" / ")
                            : modelData.key === "credit" ? row.credit : row.value(modelData.key)
                        textFormat: links ? Text.StyledText : Text.PlainText; elide: Text.ElideRight
                        linkColor: hoveredLink !== "" ? Theme.color.accentHover : Theme.color.accent
                        color: Theme.color.textSecondary; font.pointSize: Type.body
                        onLinkActivated: link => table.activated({kind:"composer", name:row.composers[Number(link)] || ""})
                        HoverHandler { cursorShape: parent.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor }
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
