import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// One row of the search flyout (design/03 SearchFlyout): a section label,
// the top result -- a large portrait and a play button -- or an artist, a
// record or a song, with its cover, what it is and, for a song, its time.
// Its menu opens with the right button or with the button at its end. The
// row takes every press on it, so none is also a press on what the flyout
// covers.
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
    // What the row is, drawn over the placeholder while it has no cover.
    property string glyph
    // The button at its end: the menu's, unless the list gives it another.
    property string endGlyph: Icons.dots
    property string endTip: I18n.tr("More", I18n.rev)
    property bool endOn: false

    signal activated()
    signal played()
    // Its menu was asked for, at that point of the row.
    signal menu(real left, real top)
    // The button at its end was pressed, when it is not the menu's.
    signal endClicked()

    readonly property bool header: kind === 0
    readonly property bool best: kind === 1
    readonly property bool person: kind === 1 || kind === 2
    // The way to the rest of a source's results.
    readonly property bool link: kind === 5

    height: header || link ? Metrics.searchHeader : best ? Metrics.searchBest : Metrics.searchRow

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
        color: area.containsMouse || hit.current ? Theme.color.stateHover : hit.best ? Theme.color.fillField : "transparent"
    }

    ArtTile {
        id: cover
        visible: !hit.header && !hit.link
        x: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        width: hit.link ? 0 : hit.best ? Metrics.searchAvatar : Metrics.searchArt
        height: width
        radius: hit.person ? width / 2 : Metrics.radiusArtSmall // derived: a circle
        source: hit.art
        tint: hit.index
    }

    Icon {
        anchors.centerIn: cover
        visible: hit.glyph !== "" && !cover.ready
        glyph: hit.glyph
        size: Metrics.iconNav
        color: Theme.color.accent
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
            visible: !hit.link
            text: hit.title
            elide: Text.ElideRight
            font.pointSize: hit.best ? Type.bestResult : Type.body
            font.weight: hit.best ? Type.semibold : Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            visible: text !== ""
            text: hit.detail
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: hit.link ? Theme.color.accent : Theme.color.textSecondary
        }
    }

    Item {
        id: tail
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        width: more.width + Metrics.gap6 + (hit.best ? Metrics.button32 : time.implicitWidth)
        height: parent.height
        z: 1

        Text {
            id: time
            anchors.right: more.left
            anchors.rightMargin: Metrics.gap6
            anchors.verticalCenter: parent.verticalCenter
            visible: !hit.best && !hit.header && !hit.link
            text: hit.duration
            font.pointSize: Type.small
            font.features: { "tnum": 1 }
            color: Theme.color.textTertiary
        }
        IconButton {
            anchors.right: more.left
            anchors.rightMargin: Metrics.gap6
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
        IconButton {
            id: more
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: !hit.header && !hit.link
            size: Metrics.button28
            glyph: hit.endGlyph
            tip: hit.endTip
            filled: hit.endOn
            tone: hit.endOn ? "accent" : "secondary"
            onClicked: hit.endGlyph === Icons.dots ? hit.menu(tail.x + more.x + more.width - Metrics.menuWidth, tail.y + more.y + more.height + Metrics.gap4) : hit.endClicked()
        }
    }

    MouseArea {
        id: area
        anchors.fill: parent
        enabled: !hit.header
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: (mouse) => mouse.button === Qt.RightButton && !hit.link ? hit.menu(mouse.x, mouse.y) : hit.activated()
    }
}
