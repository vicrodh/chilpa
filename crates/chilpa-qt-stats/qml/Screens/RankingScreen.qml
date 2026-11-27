import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.stats
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"
import "../Components"

// Every artist, record or genre of a span of days, most played first, a
// page at a time: its place, its picture, its plays and the time it was
// heard. The route's key says which and the span, as "albums", the first
// day and the last, apart by U+001F; both can be changed here, and how
// many a page shows is kept. An artist opens its page, a record its own
// and a genre the genres.
Item {
    id: ranking

    property string routeKey
    readonly property var kinds: ["artists", "albums", "genres"]
    readonly property var parts: routeKey.split("\u001f")
    property string kind: kinds.indexOf(parts[0]) >= 0 ? parts[0] : "artists"
    property string from: parts[1] || ""
    property string to: parts[2] || ""
    readonly property var sizes: [20, 50, 100]
    property int perPage: sizes.indexOf(Number(SettingsModel.kept("ui.ranking.perPage"))) >= 0 ? Number(SettingsModel.kept("ui.ranking.perPage")) : 50
    property int at: 1
    readonly property var doc: JSON.parse(Stats.ranking(kind, from, to, at, perPage, I18n.rev + Library.contentRev + Library.remoteRev + Library.remoteArtRev) || "{}")
    readonly property var rows: doc.rows || []
    readonly property real wide: Math.min(width - 2 * Metrics.contentX, Charts.widest) // derived: padding on both sides
    readonly property color tint: kind === "albums" ? Theme.color.statsAlbums : kind === "genres" ? Theme.color.accent : Theme.color.statsArtists

    function show(next) {
        at = next
        list.contentY = 0
    }
    function open(row) {
        if (kind === "artists")
            Navigation.openArtist(row.name)
        else if (kind === "albums")
            Navigation.go("album", row.id)
        else
            Navigation.goKey("genres", JSON.stringify({ kind: "genre", name: row.name, trail: [row.name] }))
    }

    Item {
        id: head
        x: (ranking.width - ranking.wide) / 2 // derived: centered past its widest
        y: Metrics.contentTop
        width: ranking.wide
        height: Metrics.shelfHeader

        Text {
            id: title
            anchors.verticalCenter: parent.verticalCenter
            text: ranking.kind === "albums" ? I18n.tr("Top albums", I18n.rev) : ranking.kind === "genres" ? I18n.tr("Top genres", I18n.rev) : I18n.tr("Top artists", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        Text {
            objectName: "rankingTotal"
            anchors.left: title.right
            anchors.leftMargin: Metrics.gap12
            anchors.baseline: title.baseline
            text: ranking.kind === "albums" ? I18n.trCount("{} album", "{} albums", ranking.doc.total || 0, I18n.rev)
                : ranking.kind === "genres" ? I18n.trCount("{} genre", "{} genres", ranking.doc.total || 0, I18n.rev)
                : I18n.trCount("{} artist", "{} artists", ranking.doc.total || 0, I18n.rev)
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap12

            DateRange {
                objectName: "rankingRange"
                anchors.verticalCenter: parent.verticalCenter
                from: ranking.from
                to: ranking.to
                latest: ranking.doc.today || ""
                onChosen: (from, to) => {
                    ranking.from = from
                    ranking.to = to
                    ranking.show(1)
                }
            }
            SegmentedControl {
                objectName: "rankingKind"
                anchors.verticalCenter: parent.verticalCenter
                options: [{ text: I18n.tr("Artists", I18n.rev) }, { text: I18n.tr("Albums", I18n.rev) }, { text: I18n.tr("Genres", I18n.rev) }]
                current: ranking.kinds.indexOf(ranking.kind)
                onActivated: (index) => {
                    ranking.kind = ranking.kinds[index]
                    ranking.show(1)
                }
            }
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
        ScrollBar.vertical: ViewportScrollBar { gutter: (ranking.width - list.x - list.width) / 2 } // derived: the middle of the room to its right

        Column {
            id: column
            width: list.width
            spacing: Metrics.gap2

            Repeater {
                model: ranking.rows

                RankRow {
                    required property var modelData
                    required property int index
                    objectName: "rankingRow" + index
                    width: column.width
                    rank: modelData.rank || index + 1
                    rankWidth: Metrics.gap24 + Metrics.gap8 // derived: room for a place in the hundreds
                    title: modelData.name || modelData.title || ""
                    detail: modelData.artist || ""
                    art: modelData.art || ""
                    plays: modelData.plays || 0
                    most: ranking.doc.most || 1
                    note: modelData.listened || ""
                    round: ranking.kind === "artists"
                    clickable: ranking.kind !== "albums" || (modelData.id || 0) > 0
                    tint: ranking.tint
                    onClicked: ranking.open(modelData)
                }
            }
        }
        Text {
            anchors.centerIn: parent
            visible: ranking.rows.length === 0
            text: I18n.tr("Nothing in these days.", I18n.rev)
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
            objectName: "rankingPager"
            anchors.verticalCenter: parent.verticalCenter
            page: ranking.doc.page || 1
            pages: ranking.doc.pages || 1
            onChosen: (next) => ranking.show(next)
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
                objectName: "rankingSize"
                anchors.verticalCenter: parent.verticalCenter
                options: ranking.sizes.map(n => ({ text: String(n) }))
                current: ranking.sizes.indexOf(ranking.perPage)
                onActivated: (index) => {
                    ranking.perPage = ranking.sizes[index]
                    SettingsModel.keep("ui.ranking.perPage", String(ranking.perPage))
                    ranking.show(1)
                }
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
