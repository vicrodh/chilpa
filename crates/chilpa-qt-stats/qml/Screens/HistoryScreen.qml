import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.stats
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as Kit
import "../Theme"
import "../Components"

// The listening history, listen by listen, newest first and under the
// day each fell on, in fixed columns, rich or compact as the track lists
// are: a page of 20, 50 or 100 at a time, the pages below it and, apart
// from them, how many a page shows, which is kept. The search keeps the
// listens whose track, artist or record hold every word.
Item {
    id: history

    readonly property var sizes: [20, 50, 100]
    property int perPage: sizes.indexOf(Number(SettingsModel.kept("ui.history.perPage"))) >= 0 ? Number(SettingsModel.kept("ui.history.perPage")) : 20
    property int at: 1
    property string query
    readonly property bool rich: SettingsModel.density === "rich"
    readonly property var doc: JSON.parse(Stats.history(at, perPage, query, I18n.rev) || "{}")
    readonly property var rows: doc.rows || []
    readonly property real wide: Math.min(width - 2 * Metrics.contentX, Charts.widest) // derived: padding on both sides

    function show(next) {
        at = next
        list.contentY = 0
    }
    function size(index) {
        perPage = sizes[index]
        SettingsModel.keep("ui.history.perPage", String(perPage))
        show(1)
    }

    Item {
        id: head
        x: (history.width - history.wide) / 2 // derived: centered past its widest
        y: Metrics.contentTop
        width: history.wide
        height: Metrics.shelfHeader

        Text {
            id: title
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Listening history", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        Text {
            objectName: "historyTotal"
            anchors.left: title.right
            anchors.leftMargin: Metrics.gap12
            anchors.baseline: title.baseline
            text: I18n.trCount("{} listen", "{} listens", history.doc.total || 0, I18n.rev)
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Kit.SearchField {
            objectName: "historySearch"
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: Charts.historySearch
            placeholder: I18n.tr("Search listens", I18n.rev)
            onTextChanged: {
                history.query = text
                history.show(1)
            }
        }
    }
    HistoryHeader {
        id: columns
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        x: head.x
        width: head.width
    }
    Flickable {
        id: list
        anchors.top: columns.bottom
        anchors.topMargin: Metrics.gap4
        anchors.bottom: foot.top
        anchors.bottomMargin: Metrics.gap8
        x: head.x
        width: head.width
        contentHeight: column.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar { gutter: (history.width - list.x - list.width) / 2 } // derived: the middle of the room to its right

        Column {
            id: column
            width: list.width

            Repeater {
                model: history.rows

                Column {
                    id: entry
                    required property var modelData
                    required property int index
                    width: column.width

                    Text {
                        visible: entry.index === 0 || history.rows[entry.index - 1].day !== entry.modelData.day
                        height: visible ? (history.rich ? Charts.dayHeader : Charts.dayHeaderCompact) : 0
                        verticalAlignment: Text.AlignBottom
                        bottomPadding: Metrics.gap6
                        text: entry.modelData.day || ""
                        font.pointSize: Type.label
                        font.weight: Type.semibold
                        font.letterSpacing: Type.labelTracking
                        font.capitalization: Font.AllUppercase
                        color: Theme.color.textSecondary
                    }
                    HistoryRow {
                        objectName: "historyRow" + entry.index
                        width: parent.width
                        listen: entry.modelData
                        index: entry.index
                        rich: history.rich
                        onOpened: (album) => Navigation.go("album", album)
                    }
                }
            }
        }
        Text {
            anchors.centerIn: parent
            visible: history.rows.length === 0
            text: history.query === "" ? I18n.tr("Nothing played yet.", I18n.rev) : I18n.tr("No listens match.", I18n.rev)
            font.pointSize: Type.body
            color: Theme.color.textTertiary
        }
    }
    Item {
        id: foot
        x: head.x
        width: head.width
        anchors.bottom: parent.bottom
        // Clear of the player bar, as tall as it stands (ListenList's).
        anchors.bottomMargin: Metrics.playerSpace + Metrics.playerBottom + Metrics.gap16
        height: Charts.pagerButton

        Pager {
            objectName: "historyPager"
            anchors.verticalCenter: parent.verticalCenter
            page: history.doc.page || 1
            pages: history.doc.pages || 1
            onChosen: (next) => history.show(next)
        }
        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap8

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Show", I18n.rev)
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
            SegmentedControl {
                objectName: "historySize"
                anchors.verticalCenter: parent.verticalCenter
                options: history.sizes.map(n => ({ text: String(n) }))
                current: history.sizes.indexOf(history.perPage)
                onActivated: (index) => history.size(index)
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("results", I18n.rev)
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
        }
    }
}
