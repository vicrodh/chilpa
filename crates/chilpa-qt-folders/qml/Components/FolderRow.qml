import QtQuick
import app.chilpa
import app.chilpa.folders
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A folder of the library: its name and path, when it was last scanned,
// whether it is there now, and its actions.
Item {
    id: row

    required property string path
    required property string name
    required property var lastScan
    required property bool reachable

    readonly property bool scanning: LibraryFolders.scanning
        && (LibraryFolders.scanningPath === "" || LibraryFolders.scanningPath === path)

    signal rescan()
    signal forget()

    implicitHeight: Metrics.folderRow

    Rectangle {
        anchors.fill: parent
        radius: Metrics.radiusControl
        color: hover.hovered ? Theme.color.stateHover : "transparent"
    }

    Icon {
        id: icon
        x: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.folder
        size: Metrics.iconMd
    }

    Column {
        anchors.left: icon.right
        anchors.leftMargin: Metrics.gap10
        anchors.right: scanned.left
        anchors.rightMargin: Metrics.gap16
        anchors.verticalCenter: parent.verticalCenter

        Text { width: parent.width; text: row.name; elide: Text.ElideRight; font.pointSize: Type.body; font.weight: Type.medium; color: Theme.color.textPrimary }
        Text { width: parent.width; text: row.path; elide: Text.ElideMiddle; font.pointSize: Type.small; color: Theme.color.textTertiary }
    }

    Text {
        id: scanned
        anchors.right: status.left
        anchors.rightMargin: Metrics.gap16
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.folderDateWidth
        text: row.scanning ? I18n.tr("Scanning…", I18n.rev)
            : row.lastScan ? Qt.formatDateTime(row.lastScan, "yyyy-MM-dd hh:mm") : "—"
        font.pointSize: Type.row
        font.features: { "tnum": 1 }
        color: row.scanning ? Theme.color.accent : Theme.color.textSecondary
    }

    Text {
        id: status
        anchors.right: actions.left
        anchors.rightMargin: Metrics.gap16
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.folderStatusWidth
        text: row.reachable ? I18n.tr("Active", I18n.rev) : I18n.tr("Missing", I18n.rev)
        font.pointSize: Type.row
        color: row.reachable ? Theme.color.textSecondary : Theme.color.statusNegative
    }

    Row {
        id: actions
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap4

        IconButton {
            tip: I18n.tr("Scan again", I18n.rev)
            glyph: Icons.refresh
            enabled: row.reachable && !LibraryFolders.scanning
            onClicked: row.rescan()
        }
        IconButton {
            tip: I18n.tr("Remove from the library", I18n.rev)
            glyph: Icons.trash
            onClicked: row.forget()
        }
    }

    HoverHandler { id: hover }
}
