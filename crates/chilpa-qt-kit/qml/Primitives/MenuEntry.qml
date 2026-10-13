import QtQuick
import "../Theme"

// One line of a Menu: a check, the text with its detail below, a mark and
// a hint on the right; a separator; or a note, a heading nothing happens on.
// It asks for the width its text needs; if the menu cannot give it, the text
// is cut, never spilled, and the whole of it shows in a tooltip.
Item {
    id: line

    property var entry: ({})
    enabled: entry.enabled !== false
    opacity: enabled ? 1 : Metrics.disabledOpacity

    signal clicked()
    readonly property bool hovered: hover.hovered

    readonly property bool separator: entry.separator === true
    readonly property bool note: entry.note === true
    readonly property bool cut: title.truncated || detail.truncated

    implicitWidth: separator ? 0
        : Metrics.gap10 * 2 + check.width + Metrics.gap8 + Math.max(title.implicitWidth, detail.implicitWidth) // derived: padding, the check and the text
          + (tail.width > 0 ? tail.width + Metrics.gap16 : 0) + sourceBadge.width
    implicitHeight: separator ? Metrics.gap8 + 1 : entry.detail ? Metrics.menuItemTall : Metrics.menuItem

    Rectangle {
        visible: line.separator
        anchors.verticalCenter: parent.verticalCenter
        x: Metrics.gap6
        width: parent.width - Metrics.gap12
        height: 1
        color: Theme.color.strokeHairline
    }

    Rectangle {
        visible: !line.separator
        anchors.fill: parent
        radius: Metrics.radiusItem
        color: line.entry.focused ? Theme.color.stateSelected : hover.hovered && !line.note ? Theme.color.stateHover : "transparent"
    }

    Icon {
        id: check
        visible: !line.separator && !line.entry.leadingSourceIcon
        x: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.menuIconColumn
        glyph: line.entry.checked ? Icons.check : line.entry.glyph || ""
        filled: line.entry.filled === true
        iconFamily: line.entry.checked ? "" : line.entry.family || ""
        size: Metrics.iconNav
        color: line.entry.checked || line.entry.accent === true ? Theme.color.accent : Theme.color.textSecondary
    }
    SourceIcon {
        anchors.centerIn: check
        visible: !!line.entry.leadingSourceIcon && !line.separator
        sourceInfo: ({icon: line.entry.leadingSourceIcon || ""})
        width: Metrics.iconNav
        height: Metrics.iconNav
    }

    SourceIcon {
        id: sourceBadge
        anchors.left: check.right
        anchors.verticalCenter: parent.verticalCenter
        sourceInfo: line.entry.sourceInfo || ({})
        visible: !!line.entry.sourceInfo && !line.separator
        width: visible ? implicitWidth : 0
    }

    Column {
        visible: !line.separator
        anchors.left: sourceBadge.right
        anchors.leftMargin: Metrics.gap8
        anchors.right: tail.left
        anchors.rightMargin: tail.width > 0 ? Metrics.gap16 : 0
        anchors.verticalCenter: parent.verticalCenter

        Text {
            id: title
            width: parent.width
            text: line.entry.text || ""
            elide: Text.ElideRight
            font.pointSize: line.note ? Type.small : Type.body
            font.weight: line.note ? Type.semibold : Type.regular
            color: line.note ? Theme.color.textTertiary : Theme.color.textPrimary
        }
        Text {
            id: detail
            width: parent.width
            visible: !!line.entry.detail
            text: line.entry.detail || ""
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
    }

    Row {
        id: tail
        visible: !line.separator
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap8

        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: !!line.entry.mark
            text: line.entry.mark || ""
            font.pointSize: Type.micro
            font.weight: Type.bold
            color: Theme.color.accent
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: !!line.entry.hint
            text: line.entry.hint || ""
            font.pointSize: Type.label
            color: Theme.color.textTertiary
        }
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: line.entry.submenu === true
            glyph: Icons.chevronRight
            size: Metrics.iconSm
            color: Theme.color.textTertiary
        }
    }

    HoverHandler {
        id: hover
        enabled: !line.separator
        onHoveredChanged: hovered && line.cut
            ? TipState.show([line.entry.text, line.entry.detail].filter(t => !!t).join(" — "), line)
            : TipState.hide(line)
    }
    HoverHandler { enabled: !line.separator && !line.note; cursorShape: Qt.PointingHandCursor }
    TapHandler {
        enabled: !line.separator && !line.note
        // The click is the entry's alone: left to pass, it is also a click
        // on whatever the menu is drawn over.
        gesturePolicy: TapHandler.ReleaseWithinBounds
        onTapped: line.clicked()
    }
}
