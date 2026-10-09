import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.stats
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"
import "../Components"

// The listening history, listen by listen, newest first and under the
// day each fell on: a page of 20, 50 or 100 at a time, the pages below it
// and, apart from them, how many a page shows, which is kept.
Item {
    id: history

    readonly property var sizes: [20, 50, 100]
    property int perPage: sizes.indexOf(Number(SettingsModel.kept("ui.history.perPage"))) >= 0 ? Number(SettingsModel.kept("ui.history.perPage")) : 20
    property int at: 1
    readonly property var doc: JSON.parse(Stats.history(at, perPage, I18n.rev) || "{}")
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
    }
    Flickable {
        id: list
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: foot.top
        anchors.bottomMargin: Metrics.gap8
        x: head.x
        width: head.width
        contentHeight: column.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar {}

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
                        height: visible ? Charts.dayHeader : 0
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
                        width: parent.width
                        listen: entry.modelData
                        index: entry.index
                        onOpened: (album) => Navigation.go("album", album)
                    }
                }
            }
        }
        Text {
            anchors.centerIn: parent
            visible: history.rows.length === 0
            text: I18n.tr("Nothing played yet.", I18n.rev)
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
