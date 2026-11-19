import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"
import "../Components"

// The recap's card: whose month or year it is, the first artist, record
// and track with the four behind each, and nine numbers, three across: the
// minutes, the plays and the plays a day; how many tracks, records and
// artists; and how many of each were heard for the first time. A period
// with too little in it says so instead.
Rectangle {
    id: card

    property var recap: ({})
    property bool timed: false
    readonly property bool empty: recap.empty !== false
    readonly property int across: body.width >= 3 * Charts.recapCell ? 3 : 1 // derived: three figures across while they fit

    function whole(value) {
        return Number(value || 0).toLocaleString(Qt.locale(), "f", 0)
    }

    implicitHeight: (empty ? nothing.implicitHeight : body.implicitHeight) + 2 * Charts.recapPad // derived: the padding above and below
    radius: Metrics.radiusPanel
    color: Theme.ambient ? Theme.color.ambientPanel : Theme.color.surfacePanel
    border.width: 1
    border.color: Theme.color.strokeHairline

    Text {
        id: nothing
        objectName: "recapNothing"
        visible: card.empty
        x: Charts.recapPad
        y: Charts.recapPad
        width: card.width - 2 * Charts.recapPad // derived: inside the padding
        wrapMode: Text.WordWrap
        text: card.recap.any === false ? I18n.tr("Nothing played yet.", I18n.rev) : I18n.tr("Not enough was played in this period for a recap.", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textTertiary
    }
    Column {
        id: body
        visible: !card.empty
        x: Charts.recapPad
        y: Charts.recapPad
        width: card.width - 2 * Charts.recapPad // derived: inside the padding
        spacing: Metrics.gap24

        Column {
            width: parent.width
            spacing: Metrics.gap8

            Text {
                objectName: "recapTitle"
                width: parent.width
                text: card.recap.title || ""
                wrapMode: Text.WordWrap
                font.pointSize: Charts.recapTitle
                font.weight: Type.semibold
                font.letterSpacing: Type.titleTracking
                color: Theme.color.textPrimary
            }
            Rectangle {
                width: Charts.recapRuleWide
                height: Charts.recapRule
                color: Theme.color.accent
            }
        }
        RecapTop {
            objectName: "recapArtist"
            width: parent.width
            caption: I18n.tr("Top artist", I18n.rev)
            leaders: card.recap.artists || []
            round: true
            timed: card.timed
            clickable: true
            onOpened: Navigation.openArtist(first.name)
        }
        RecapTop {
            objectName: "recapAlbum"
            width: parent.width
            caption: I18n.tr("Top album", I18n.rev)
            leaders: card.recap.albums || []
            timed: card.timed
            clickable: (first.id || 0) !== 0
            onOpened: Navigation.go("album", first.id)
        }
        RecapTop {
            objectName: "recapTrack"
            width: parent.width
            caption: I18n.tr("Top track", I18n.rev)
            leaders: card.recap.tracks || []
            timed: card.timed
        }
        Grid {
            id: figures
            objectName: "recapFigures"
            width: parent.width
            columns: card.across
            columnSpacing: Metrics.gap16
            rowSpacing: Metrics.gap16
            readonly property real cell: (width - (columns - 1) * columnSpacing) / columns

            Repeater {
                // Row by row: time and plays, how many, how many were new.
                model: [
                    { value: card.whole(card.recap.minutes), label: I18n.tr("Minutes", I18n.rev) },
                    { value: card.whole(card.recap.tracks_count), label: I18n.tr("Tracks", I18n.rev) },
                    { value: card.whole(card.recap.new_tracks), label: I18n.tr("New tracks", I18n.rev) },
                    { value: card.whole(card.recap.plays), label: I18n.tr("Plays", I18n.rev) },
                    { value: card.whole(card.recap.albums_count), label: I18n.tr("Albums", I18n.rev) },
                    { value: card.whole(card.recap.new_albums), label: I18n.tr("New albums", I18n.rev) },
                    { value: Number(card.recap.per_day || 0).toLocaleString(Qt.locale(), "f", 1), label: I18n.tr("Plays a day", I18n.rev) },
                    { value: card.whole(card.recap.artists_count), label: I18n.tr("Artists", I18n.rev) },
                    { value: card.whole(card.recap.new_artists), label: I18n.tr("New artists", I18n.rev) }
                ]

                RecapFigure {
                    required property var modelData
                    width: figures.cell
                    value: modelData.value
                    label: modelData.label
                }
            }
        }
    }
}
