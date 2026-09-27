import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// One collapsible playlist folder in the sidebar.
Item {
    id: row

    required property int index
    required property int folderId
    required property string name
    required property int tracks
    required property string folderColor
    required property bool expanded

    signal toggled()

    readonly property color tone: !SettingsModel.folderColors ? Theme.color.textSecondary
        : folderColor === "positive" ? Theme.color.statusPositive
        : folderColor === "warning" ? Theme.color.statusWarning
        : folderColor === "negative" ? Theme.color.statusNegative
        : Theme.color.accent

    implicitHeight: Metrics.playlistRowHeight

    ListRowSurface {
        anchors.fill: parent
        rowIndex: row.index
        listMode: false
        radius: Metrics.radiusControl
        hovered: hover.hovered
    }
    Icon {
        id: folderIcon
        x: Metrics.navItemPadding
        anchors.verticalCenter: parent.verticalCenter
        glyph: row.expanded ? Icons.folderOpen : Icons.folder
        size: Metrics.iconMd
        color: row.tone
    }
    Text {
        anchors.left: folderIcon.right
        anchors.leftMargin: Metrics.navItemPadding
        anchors.right: count.left
        anchors.rightMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        text: row.name
        elide: Text.ElideRight
        font.pointSize: Type.body
        font.weight: Type.medium
        color: Theme.color.textPrimary
    }
    Text {
        id: count
        anchors.right: chevron.left
        anchors.rightMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        text: row.tracks
        font.pointSize: Type.small
        color: Theme.color.textTertiary
    }
    Icon {
        id: chevron
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        glyph: row.expanded ? Icons.chevronUp : Icons.chevronDown
        size: Metrics.iconSm
        color: Theme.color.textTertiary
    }
    HoverHandler { id: hover }
    TapHandler { onTapped: row.toggled() }
}
