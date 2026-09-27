import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// One row of the search flyout (design/03 SearchFlyout): a section label,
// the top result -- a large portrait and a play button -- or an artist, a
// record or a song, with its cover, what it is and, for a song, its time.
Item {
    id: hit

    required property int index
    required property int kind
    required property string title
    required property string detail
    required property url art
    required property string duration
    // The row the keyboard is on.
    property bool current: false

    signal activated()
    signal played()

    readonly property bool header: kind === 0
    readonly property bool best: kind === 1
    readonly property bool person: kind === 1 || kind === 2

    height: header ? Metrics.searchHeader : best ? Metrics.searchBest : Metrics.searchRow

    Text {
        visible: hit.header
        x: Metrics.gap8
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap6
        text: hit.title
        font.pointSize: Type.small
        font.weight: Type.semibold
        color: Theme.color.textTertiary
    }

    Rectangle {
        visible: !hit.header
        anchors.fill: parent
        radius: Metrics.radiusCard
        color: hover.hovered || hit.current ? Theme.color.stateHover : hit.best ? Theme.color.fillField : "transparent"
    }

    ArtTile {
        id: cover
        visible: !hit.header
        x: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        width: hit.best ? Metrics.searchAvatar : Metrics.searchArt
        height: width
        radius: hit.person ? width / 2 : Metrics.radiusArtSmall // derived: a circle
        source: hit.art
        tint: hit.index
    }

    Column {
        visible: !hit.header
        anchors.left: cover.right
        anchors.leftMargin: hit.best ? Metrics.gap12 : Metrics.gap10
        anchors.right: tail.left
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter

        Text {
            width: parent.width
            text: hit.title
            elide: Text.ElideRight
            font.pointSize: hit.best ? Type.bestResult : Type.body
            font.weight: hit.best ? Type.semibold : Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: hit.detail
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
    }

    Item {
        id: tail
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        width: hit.best ? Metrics.button32 : time.implicitWidth
        height: parent.height

        Text {
            id: time
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: !hit.best && !hit.header
            text: hit.duration
            font.pointSize: Type.small
            font.features: { "tnum": 1 }
            color: Theme.color.textTertiary
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            visible: hit.best
            size: Metrics.button32
            round: true
            filled: true
            tone: "solid"
            glyph: Icons.playerPlayFilled
            tip: I18n.tr("Play", I18n.rev)
            onClicked: hit.played()
        }
    }

    HoverHandler { id: hover; enabled: !hit.header }
    HoverHandler { enabled: !hit.header; cursorShape: Qt.PointingHandCursor }
    TapHandler { enabled: !hit.header; onTapped: hit.activated() }
}
