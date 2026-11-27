import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// One name of a top list: its place, its picture, what it is and whose,
// its plays (and, given, the time it was heard), and a thin bar for how
// near it came to the first. The first is drawn larger; an artist's
// picture is round.
Item {
    id: row

    property int rank: 1
    property string title
    property string detail
    property string art
    property int plays: 0
    property int most: 1
    property bool round: false
    property bool lead: false
    property bool clickable: true
    property color tint: Theme.color.accent
    property string note
    // A genre has no picture, and no empty square takes its place.
    property bool pictured: true
    // Room for the place: wider in a ranking that runs to hundreds.
    property real rankWidth: Metrics.gap16
    readonly property int side: lead ? Charts.thumbLead : Charts.thumb

    signal clicked()

    implicitHeight: side + 2 * Metrics.gap4 // derived: room above and below

    Rectangle {
        anchors.fill: parent
        radius: Metrics.radiusItem
        color: area.containsMouse && row.clickable ? Theme.color.stateHover : "transparent"
    }
    Text {
        id: place
        width: row.rankWidth
        anchors.verticalCenter: parent.verticalCenter
        horizontalAlignment: Text.AlignRight
        text: row.rank
        font.pointSize: Type.small
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }
    ArtTile {
        id: cover
        x: place.width + Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        visible: row.pictured
        width: row.pictured ? row.side : 0
        height: row.side
        path: row.art
        radius: row.round ? row.side / 2 : Metrics.radiusItem // derived: a circle for an artist
        tint: row.rank
    }
    Column {
        anchors.left: cover.right
        anchors.leftMargin: row.pictured ? Metrics.gap10 : 0
        anchors.right: count.left
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2

        Text {
            width: parent.width
            text: row.title
            elide: Text.ElideRight
            font.pointSize: row.lead ? Type.bodyLarge : Type.body
            font.weight: row.lead ? Type.semibold : Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            visible: text !== ""
            text: row.detail
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
        Rectangle {
            width: Math.max(Charts.rankBar, parent.width * row.plays / Math.max(1, row.most))
            height: Charts.rankBar
            radius: height / 2 // derived: rounded ends
            color: row.tint
        }
    }
    Text {
        id: count
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap4
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.trCount("{} play", "{} plays", row.plays, I18n.rev) + (row.note !== "" ? " · " + row.note : "")
        font.pointSize: Type.small
        font.features: { "tnum": 1 }
        color: Theme.color.textSecondary
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        enabled: row.clickable
        cursorShape: Qt.PointingHandCursor
        onClicked: row.clicked()
    }
}
