// hot-path: one per album in a grid, recycled with its row.
import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// A record: its cover, its title and a subtitle (design/03 AlbumCard). The
// hover overlay is built only for the card under the pointer. A selected
// card has an accent ring and one with its quick view open a quiet one;
// while anything is selected every check shows, since a click selects.
Item {
    id: card
    implicitWidth: CardLayout.cardWidth

    // Every source that keeps the record, its own first: one card can stand
    // for the copies several sources keep.
    property var sources: []
    readonly property var sourceInfo: sources.length > 0 ? sources[0] : ({})
    property var qualityInfo: ({})
    property int albumId
    property bool explicitContent: false
    property string title
    property string subtitle
    property string artistName: subtitle
    property var artistDestination: ({})
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
        anchors.right: cover.right
        anchors.top: cover.top
        anchors.margins: Metrics.overlayPad
        width: Metrics.overlayCheck; height: Metrics.overlayCheck
        visible: card.selecting && !hover.hovered
        radius: width / 2 // derived: a circle
        color: Theme.color.overlayScrim

        Icon {
            anchors.centerIn: parent
            glyph: card.selected ? Icons.circleCheckFilled : Icons.circle
            filled: card.selected
            size: Metrics.overlayCheckIcon
            color: Theme.color.backdropText
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
        width: badges.width; height: badges.height
        anchors.right: cover.right; anchors.bottom: cover.bottom; anchors.margins: Metrics.gap4
        visible: card.sources.length > 0
        Rectangle {
            anchors.fill: badges
            anchors.margins: -Metrics.gap2
            radius: Metrics.radiusCard
            color: Theme.color.shadowElevatedKey
            opacity: 0.7
        }
        Row {
            id: badges
            objectName: "sourceBadges"
            spacing: Metrics.gap4
            Repeater {
                objectName: "sourceBadgeIcons"
                // Every source the record is on, as many as fit the cover.
                model: card.sources.slice(0, Math.max(1, Math.floor((cover.width - Metrics.gap8) / (Metrics.iconSm + Metrics.gap8))))
                SourceIcon {
                    required property var modelData
                    width: Metrics.iconSm + Metrics.gap4; height: width
                    sourceInfo: modelData
                    color: Theme.color.backdropText
                    opacity: 0.84
                }
            }
        }
    }
    Column {
        anchors.top: cover.bottom
        anchors.topMargin: Metrics.cardTextGap
        width: card.width

        AdvisoryTitle {
            explicitContent: card.explicitContent
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
        AlbumArtistText {
            id: artistLabel
            width: parent.width
            text: card.subtitle
            albumId: card.albumId; artist: card.artistName; destination: card.artistDestination
            navigable: !card.round && !!card.artistName
            onFallback: card.clicked(0)
            horizontalAlignment: card.round ? Text.AlignHCenter : Text.AlignLeft
            elide: Text.ElideRight; font.pointSize: Type.meta
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
            const at = artistLabel.mapFromItem(card, point.position.x, point.position.y)
            if (button === Qt.LeftButton && artistLabel.navigable && at.x >= 0 && at.x <= artistLabel.width && at.y >= 0 && at.y <= artistLabel.height) return
            if (button === Qt.RightButton && !card.purchaseMode)
                return card.menu(point.position.x, point.position.y)
            if (button === Qt.RightButton)
                return
            const held = tap.point.modifiers
            card.clicked((held & (Qt.ControlModifier | Qt.MetaModifier) ? 1 : 0) | (held & Qt.ShiftModifier ? 2 : 0))
        }
    }
}
