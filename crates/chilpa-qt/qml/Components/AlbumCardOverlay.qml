import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// What a card shows under the pointer (design/03 AlbumCard): the scrim, the
// check that selects it, its favorite, play, its quick view and its menu.
// Only one exists at a time, so it can afford what a card cannot.
Rectangle {
    id: overlay

    property int albumId
    property string year
    property bool selected
    property bool expanded
    // Only where a grid can open one under the row (ADR-0007).
    property bool canExpand: true
    readonly property bool favorite: Ratings.favorite("album", albumId, Ratings.rev)
    readonly property var sources: JSON.parse(AlbumActions.sources(albumId, Library.contentRev) || "[]")
    readonly property var information: JSON.parse(AlbumActions.information(albumId, Library.contentRev + Library.remoteRev) || "{}")
    readonly property string genre: information.genre || ""
    // "April 26, 1981" when the record's files or its source say the day.
    readonly property string released: information.released || year

    signal select()
    signal quick()
    signal menu(real x, real y)

    radius: Metrics.radiusCard
    color: Theme.color.overlayScrim

    Row {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.margins: Metrics.overlayPad
        spacing: Metrics.gap6

        Item {
            id: selector
            width: Metrics.overlayCheck
            height: Metrics.overlayCheck

            Icon {
                anchors.centerIn: parent
                glyph: overlay.selected ? Icons.circleCheckFilled : Icons.circle
                filled: overlay.selected
                size: Metrics.overlayCheckIcon
                color: Theme.color.textOnAccent
            }
            MouseArea {
                cursorShape: Qt.PointingHandCursor
                anchors.fill: parent
                hoverEnabled: true
                onClicked: overlay.select()
                onContainsMouseChanged: containsMouse ? TipState.show(I18n.tr("Select", I18n.rev), selector) : TipState.hide(selector)
            }
        }
        Text {
            visible: overlay.released !== "" && overlay.sources.length < 2
            anchors.verticalCenter: parent.verticalCenter
            text: overlay.released
            font.pointSize: Type.meta
            font.weight: Type.semibold
            color: Theme.color.textOnAccent
        }
    }

    Text { anchors.left: parent.left; anchors.top: parent.top; anchors.leftMargin: Metrics.overlayPad; anchors.topMargin: Metrics.overlayPad + Metrics.overlayCheck + Metrics.gap4; width: parent.width - Metrics.overlayPad - Metrics.overlayPad; visible: overlay.genre !== ""; text: overlay.genre; textFormat: Text.PlainText; wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textOnAccent }

    IconButton {
        id: favoriteButton
        // A source's record is a favorite of the service, not of the library.
        visible: overlay.albumId >= 0
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Metrics.overlayPad
        size: Metrics.overlayButton
        iconSize: Metrics.overlayIcon
        round: true
        tone: "overlay"
        checked: overlay.favorite
        glyph: overlay.favorite ? Icons.heartFilled : Icons.heart
        filled: overlay.favorite
        tip: overlay.favorite ? I18n.tr("Remove from favorites", I18n.rev) : I18n.tr("Add to favorites", I18n.rev)
        onClicked: Ratings.setFavorite("album", overlay.albumId, !overlay.favorite)
    }

    Row {
        anchors.right: favoriteButton.visible ? favoriteButton.left : parent.right
        anchors.rightMargin: favoriteButton.visible ? Metrics.gap4 : Metrics.overlayPad
        anchors.verticalCenter: favoriteButton.verticalCenter
        spacing: Metrics.gap4
        Repeater {
            model: overlay.sources.slice(0, 4)
            SourceIcon {
                required property var modelData
                sourceInfo: modelData
                color: Theme.color.textOnAccent
                HoverHandler {
                    onHoveredChanged: hovered ? TipState.show(parent.sourceInfo.label, parent) : TipState.hide(parent)
                }
            }
        }
    }

    Rectangle {
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: Metrics.overlayPad
        width: Metrics.overlayPlay
        height: Metrics.overlayPlay
        radius: width / 2 // derived: a circle
        color: button.containsMouse ? Theme.color.accentHover : Theme.color.accent

        Icon {
            anchors.centerIn: parent
            glyph: Icons.playerPlayFilled
            filled: true
            size: Metrics.iconNav
            color: Theme.color.textOnAccent
        }

        MouseArea {
            id: button
            cursorShape: Qt.PointingHandCursor
            anchors.fill: parent
            hoverEnabled: true
            onClicked: AlbumActions.play(overlay.albumId, false)
            onContainsMouseChanged: containsMouse ? TipState.show(I18n.tr("Play", I18n.rev), parent) : TipState.hide(parent)
        }
    }

    Row {
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: Metrics.overlayPad
        spacing: Metrics.gap6

        IconButton {
            visible: overlay.canExpand
            size: Metrics.overlayButton
            round: true
            tone: "overlay"
            checked: overlay.expanded
            glyph: Icons.layoutBottombarExpand
            tip: I18n.tr("Quick view", I18n.rev)
            onClicked: overlay.quick()
        }
        IconButton {
            id: more
            size: Metrics.overlayButton
            round: true
            tone: "overlay"
            glyph: Icons.dots
            tip: I18n.tr("More", I18n.rev)
            onClicked: overlay.menu(more.x + overlay.width - parent.width - Metrics.overlayPad, overlay.height)
        }
    }
}
