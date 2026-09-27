import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

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
    readonly property bool favorite: AlbumActions.favorite(albumId, AlbumActions.rev)

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
                anchors.fill: parent
                hoverEnabled: true
                onClicked: overlay.select()
                onContainsMouseChanged: containsMouse ? TipState.show(I18n.tr("Select", I18n.rev), selector) : TipState.hide(selector)
            }
        }
        Text {
            visible: overlay.year !== ""
            anchors.verticalCenter: parent.verticalCenter
            text: overlay.year
            font.pointSize: Type.meta
            font.weight: Type.semibold
            color: Theme.color.textOnAccent
        }
    }

    IconButton {
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
        onClicked: AlbumActions.setFavorite(overlay.albumId, !overlay.favorite)
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
