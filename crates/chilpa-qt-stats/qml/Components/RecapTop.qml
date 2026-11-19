import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// The first of a kind in the recap -- the artist, the record or the track
// most played -- with its picture and its plays, and the four behind it as
// lines. With `timed`, each says the minutes it sounded too.
Item {
    id: entry

    // What it is the first of: "Top artist".
    property string caption
    // [{name | title, art, plays, minutes}], the first of them drawn large.
    property var leaders: []
    property bool round: false
    property bool timed: false
    property bool clickable: false
    readonly property var first: leaders[0] || ({})

    signal opened()

    function named(row) {
        return row.name || row.title || ""
    }
    function counted(row) {
        const plays = I18n.trCount("{} play", "{} plays", row.plays || 0, I18n.rev)
        return entry.timed ? I18n.trArgs("{} · {}", [plays, I18n.trArgs("{} min", [String(row.minutes || 0)], I18n.rev)], I18n.rev) : plays
    }

    implicitHeight: Charts.recapCover

    ArtTile {
        id: cover
        width: Charts.recapCover
        height: Charts.recapCover
        path: entry.first.art || ""
        radius: entry.round ? width / 2 : Metrics.radiusCard // derived: a circle for an artist
    }
    Column {
        anchors.left: cover.right
        anchors.leftMargin: Metrics.gap16
        anchors.right: parent.right
        anchors.verticalCenter: cover.verticalCenter
        spacing: Metrics.gap2

        Text {
            text: entry.caption
            font.pointSize: Type.label
            font.weight: Type.semibold
            font.letterSpacing: Type.labelTracking
            font.capitalization: Font.AllUppercase
            color: Theme.color.textSecondary
        }
        Text {
            id: lead
            objectName: "recapLead"
            width: parent.width
            text: entry.named(entry.first)
            elide: Text.ElideRight
            font.pointSize: text.length > Charts.recapLongName ? Charts.recapLeadLong : Charts.recapLead
            font.weight: Type.semibold
            color: area.containsMouse && entry.clickable ? Theme.color.accent : Theme.color.textPrimary

            MouseArea {
                id: area
                anchors.fill: parent
                enabled: entry.clickable
                hoverEnabled: true
                cursorShape: entry.clickable ? Qt.PointingHandCursor : Qt.ArrowCursor
                onClicked: entry.opened()
            }
        }
        Text {
            objectName: "recapLeadPlays"
            text: entry.counted(entry.first)
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
        Item {
            width: 1
            height: Metrics.gap6
        }
        Repeater {
            model: entry.leaders.slice(1)

            Text {
                required property var modelData
                width: parent.width
                text: I18n.trArgs("{} · {}", [entry.named(modelData), entry.counted(modelData)], I18n.rev)
                elide: Text.ElideRight
                font.pointSize: Type.small
                color: Theme.color.textPrimary
            }
        }
    }
}
