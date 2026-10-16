import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"
import "../Components"

// A tile of the period's artists, records or tracks: how many there were,
// in the tile's color, against the period before, and the first five,
// the first larger. An artist opens its page, a record its own.
StatsCard {
    id: tile

    // "artists", "albums" or "tracks".
    property string kind
    property var numbers: ({})
    property color tint: Theme.color.accent
    readonly property var leaders: numbers.top || []

    Row {
        id: figure
        spacing: Metrics.gap10
        Text {
            text: Number(tile.numbers.count || 0).toLocaleString(Qt.locale(), "f", 0)
            font.pointSize: Charts.tileNumber
            font.weight: Type.bold
            font.features: { "tnum": 1 }
            color: tile.tint
        }
        ChangeBadge {
            anchors.verticalCenter: parent.verticalCenter
            change: tile.numbers.change === undefined ? null : tile.numbers.change
        }
    }
    Column {
        y: figure.height + Metrics.gap12
        width: parent.width
        spacing: Metrics.gap2

        Repeater {
            model: tile.leaders

            RankRow {
                required property var modelData
                required property int index
                width: parent.width
                rank: index + 1
                lead: index === 0
                title: modelData.name || modelData.title || ""
                detail: modelData.artist || ""
                art: modelData.art || ""
                plays: modelData.plays || 0
                most: tile.leaders[0].plays || 1
                round: tile.kind === "artists"
                clickable: tile.kind !== "tracks"
                tint: tile.tint
                onClicked: tile.kind === "artists" ? Navigation.openArtist(modelData.name) : Navigation.go("album", modelData.id)
            }
        }
    }
    Text {
        anchors.centerIn: parent
        visible: tile.leaders.length === 0
        text: I18n.tr("Nothing in this period.", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textTertiary
    }
}
