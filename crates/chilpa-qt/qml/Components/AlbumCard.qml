// hot-path: one per album in a grid, recycled with its row.
import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A record: its cover, its title and a subtitle (design/03 AlbumCard). The
// hover overlay is built only for the card under the pointer. A selected
// card has an accent ring and one with its quick view open a quiet one;
// while anything is selected every check shows, since a click selects.
Item {
    id: card

    property var sourceInfo: ({})
    property var qualityInfo: ({})
    property int albumId
    property string title
    property string subtitle
    property string year
    property url art
    property var artColors: []
    property int tint: 0
    // An artist: a round portrait and the text centered under it
    // (design/03 ArtistCard).
    property bool round: false

    property bool selected: false
    property bool selecting: false
    property bool expanded: false
    property bool canExpand: true
    // A purchase catalogue uses the same record card without library actions.
    property bool purchaseMode: false
    // Nothing of it plays now: its folder is not mounted or its server does
    // not answer (ADR-017). Dimmed, and no overlay: a click says why.
    property bool available: true

    // It came in with a "Load more", at that moment: it fades in while
    // that is recent, and a recycled card that comes to stand for one later
    // does not.
    property bool fresh: false
    property real freshAt: 0
    onFreshChanged: if (fresh && Date.now() - freshAt < Motion.arrival) arrival.restart()
    Component.onCompleted: if (fresh && Date.now() - freshAt < Motion.arrival) arrival.restart()
    NumberAnimation { id: arrival; target: card; property: "opacity"; from: 0; to: card.available ? 1 : Metrics.disabledOpacity; duration: Motion.base; easing.type: Easing.OutCubic }

    // A click outside the overlay's buttons, with the modifiers held: 1 for
    // Ctrl or Cmd, 2 for Shift.
    signal clicked(int modifiers)
    signal quick()
    signal menu(real x, real y)

    implicitHeight: width + Metrics.cardTextGap + Metrics.cardTextHeight
    opacity: available ? 1 : Metrics.disabledOpacity

    ArtTile {
        id: cover
        width: card.width; height: card.width
        radius: card.round ? width / 2 : Metrics.radiusCard // derived: a circle
        source: card.art
        contain: card.sourceInfo.contain_art === true
        tint: card.tint
    }
    AnimatedCover {
        objectName: "albumAnimatedCover"
        anchors.fill: cover
        visible: !cover.ready && card.artColors.length >= 3
        colors: card.artColors
        label: card.title
    }

    Rectangle {
        anchors.fill: cover
        anchors.margins: -Metrics.cardRing
        visible: card.selected || card.expanded
        radius: Metrics.radiusCard + Metrics.cardRing // derived: around the cover's corner
        color: "transparent"
        border.width: Metrics.cardRing
        border.color: card.selected ? Theme.color.accent : Theme.color.textTertiary
    }

    Rectangle {
        x: Metrics.overlayPad; y: Metrics.overlayPad
        width: Metrics.overlayCheck; height: Metrics.overlayCheck
        visible: card.selecting && !hover.hovered
        radius: width / 2 // derived: a circle
        color: Theme.color.overlayScrim

        Icon {
            anchors.centerIn: parent
            glyph: card.selected ? Icons.circleCheckFilled : Icons.circle
            filled: card.selected
            size: Metrics.overlayCheckIcon
            color: Theme.color.textOnAccent
        }
    }

    Loader {
        anchors.fill: cover
        active: hover.hovered && !card.round && card.available && !card.purchaseMode
        sourceComponent: AlbumCardOverlay {
            albumId: card.albumId
            year: card.year
            selected: card.selected
            expanded: card.expanded
            canExpand: card.canExpand
            onSelect: card.clicked(1)
            onQuick: card.quick()
            onMenu: (x, y) => card.menu(x, y)
        }
    }

    Item {
        width: Metrics.iconSm + Metrics.gap4; height: width
        anchors.right: cover.right; anchors.bottom: cover.bottom; anchors.margins: Metrics.gap4
        visible: !!card.sourceInfo.icon && !(hover.hovered && !card.round && card.available)
        Rectangle {
            anchors.fill: sourceBadge
            anchors.margins: -Metrics.gap2
            radius: Metrics.radiusCard
            color: Theme.color.shadowElevatedKey
            opacity: 0.7
        }
        SourceIcon {
            id: sourceBadge
            anchors.fill: parent
            sourceInfo: card.sourceInfo
            opacity: 0.84
        }
    }
    Column {
        anchors.top: cover.bottom
        anchors.topMargin: Metrics.cardTextGap
        width: card.width

        Text {
            id: titleLine; objectName: "albumTitle"
            width: parent.width - (qualityBadge.visible ? qualityBadge.width + Metrics.gap4 : 0)
            text: card.title
            horizontalAlignment: card.round ? Text.AlignHCenter : Text.AlignLeft
            elide: Text.ElideRight; font.pointSize: Type.body
            font.weight: Type.medium; color: Theme.color.textPrimary
            // A name cut short says itself whole on hover.
            HoverHandler { enabled: titleLine.truncated; onHoveredChanged: hovered ? TipState.show(card.title, titleLine) : TipState.hide(titleLine) }
            Component.onDestruction: TipState.hide(titleLine)
        }
        Text {
            width: parent.width
            text: card.subtitle
            horizontalAlignment: card.round ? Text.AlignHCenter : Text.AlignLeft
            elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary
        }
    }

    QualityIcon {
        id: qualityBadge; objectName: "albumQualityBadge"
        anchors.right: parent.right; y: cover.height + Metrics.cardTextGap
        tier: card.qualityInfo.tier || 0
        visible: !card.round && !card.purchaseMode; opacity: tier > 0 ? 1 : Metrics.disabledOpacity
        HoverHandler { onHoveredChanged: hovered ? TipState.show(card.qualityInfo.label || I18n.tr("Unknown quality", I18n.rev), qualityBadge) : TipState.hide(qualityBadge) }
        Component.onDestruction: TipState.hide(qualityBadge)
    }

    HoverHandler { id: hover }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler {
        id: tap
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onTapped: (point, button) => {
            if (button === Qt.RightButton && !card.purchaseMode)
                return card.menu(point.position.x, point.position.y)
            if (button === Qt.RightButton)
                return
            const held = tap.point.modifiers
            card.clicked((held & (Qt.ControlModifier | Qt.MetaModifier) ? 1 : 0) | (held & Qt.ShiftModifier ? 2 : 0))
        }
    }
}
