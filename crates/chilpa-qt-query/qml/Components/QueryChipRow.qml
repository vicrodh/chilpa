import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.query
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The whole query on the crumb line while Refine is closed (F1): one chip
// per condition, each removable, as many as fit, then "+N more", which opens
// Refine like a click on any chip. Before them, with Refine open too, what
// the columns' headers narrow (`ColumnSearchCrumbs`), which Refine does not
// list.
Item {
    id: chips

    // The query as text; `edited` hands back the one a removal made.
    property string query: Library.queryText
    readonly property var reading: {
        I18n.rev
        Prefs.ratings
        return JSON.parse(QueryChips.refine(chips.query) || "{\"rows\":[]}")
    }
    readonly property var shown: reading.complex ? [Object.assign({ console: true }, reading.complex)] : (reading.rows || [])
    // How many chips fit before "+N more".
    property int fit: shown.length
    // Refine shows the conditions itself while it is open.
    property bool suppressed: false
    signal edited(string query)
    signal openAsked()

    // What the headers narrow shows with Refine open too.
    readonly property bool searching: searchCrumbs.searches.length > 0
    visible: searching || !suppressed && shown.length > 0
    implicitHeight: Refine.chipTall

    function measure() {
        let used = chips.searching ? searchCrumbs.implicitWidth : 0
        let count = 0
        const room = chips.width - more.implicitWidth - Refine.chipsApart
        for (let i = 0; i < repeater.count; ++i) {
            const item = repeater.itemAt(i)
            if (!item) break
            const next = used + item.implicitWidth + (used > 0 ? Refine.chipsApart : 0)
            if (next > (i === repeater.count - 1 ? chips.width : room)) break
            used = next
            count = i + 1
        }
        chips.fit = count
    }
    onWidthChanged: Qt.callLater(measure)
    onShownChanged: Qt.callLater(measure)

    Row {
        anchors.verticalCenter: parent.verticalCenter
        spacing: Refine.chipsApart
        ColumnSearchCrumbs {
            id: searchCrumbs
            anchors.verticalCenter: parent.verticalCenter
            // What is left beside "+N more" when the query's chips are there.
            room: chips.width - (!chips.suppressed && chips.shown.length > 0 ? more.implicitWidth + Refine.chipsApart : 0)
            onImplicitWidthChanged: Qt.callLater(chips.measure)
        }
        Repeater {
            id: repeater
            model: chips.shown
            SummaryChip {
                required property var modelData
                required property int index
                objectName: "summaryChip"
                visible: !chips.suppressed && index < chips.fit
                row: modelData
                asText: !!modelData.console
                onOpened: chips.openAsked()
                onRemoved: chips.edited(modelData.console ? "" : QueryChips.refineEdit(chips.query, JSON.stringify({ op: "remove", id: modelData.id })))
                onImplicitWidthChanged: Qt.callLater(chips.measure)
            }
        }
        Text {
            id: more
            objectName: "summaryMore"
            visible: !chips.suppressed && chips.fit < chips.shown.length
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.trArgs("+{} more", [String(chips.shown.length - chips.fit)], I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.accent
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: chips.openAsked()
            }
        }
    }
}
