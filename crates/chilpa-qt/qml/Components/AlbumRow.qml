// hot-path: one per record in a list of records, recycled while it scrolls.
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A record as a row: an arrow that opens its quick view under it, its cover,
// its title and who made it, and its year. A click anywhere but the arrow
// opens the record.
Item {
    id: row

    property var sourceInfo: ({})
    property int albumId
    property string title
    property string subtitle
    property string year
    property url art
    property int tint: 0

    property bool selected: false
    property bool expanded: false
    // Nothing of it plays now (ADR-017): dimmed, and a click says why.
    property bool available: true

    // A click outside the arrow, with the modifiers held: 1 for Ctrl or
    // Cmd, 2 for Shift.
    signal clicked(int modifiers)
    signal quick()
    signal menu(real x, real y)

    implicitHeight: Metrics.rowRich
    opacity: available ? 1 : Metrics.disabledOpacity

    ListRowSurface {
        anchors.fill: parent
        radius: Metrics.radiusRow
        rowIndex: row.tint
        hovered: hover.hovered
        selected: row.selected
        emphasized: row.expanded
    }

    Item {
        id: arrow
        width: Metrics.button32
        height: parent.height

        Icon {
            anchors.centerIn: parent
            glyph: Icons.chevronRight
            size: Metrics.iconSm
            color: over.hovered ? Theme.color.textPrimary : Theme.color.textSecondary
            rotation: row.expanded ? 90 : 0
        }
        HoverHandler { id: over; cursorShape: Qt.PointingHandCursor }
        TapHandler {
            enabled: row.available
            gesturePolicy: TapHandler.ReleaseWithinBounds
            onTapped: row.quick()
        }
    }
    ArtTile {
        id: cover
        anchors.left: arrow.right
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.artRow
        height: Metrics.artRow
        radius: Metrics.radiusArtSmall
        source: row.art
        tint: row.tint
    }
    SourceIcon {
        anchors.right: cover.right; anchors.bottom: cover.bottom; anchors.margins: Metrics.gap4
        visible: !!row.sourceInfo.icon; sourceInfo: row.sourceInfo
    }
    Column {
        anchors.left: cover.right
        anchors.leftMargin: Metrics.gap12
        anchors.right: when.left
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter

        Text {
            width: parent.width
            text: row.title
            elide: Text.ElideRight
            font.pointSize: Type.bodyLarge
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: row.subtitle
            elide: Text.ElideRight
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
    }
    Text {
        id: when
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        text: row.year
        font.pointSize: Type.row
        color: Theme.color.textSecondary
    }

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler {
        id: tap
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onTapped: (point, button) => {
            if (button === Qt.RightButton)
                return row.menu(point.position.x, point.position.y)
            // The arrow is its own place.
            if (point.position.x < cover.x)
                return
            const held = tap.point.modifiers
            row.clicked((held & (Qt.ControlModifier | Qt.MetaModifier) ? 1 : 0) | (held & Qt.ShiftModifier ? 2 : 0))
        }
    }
}
