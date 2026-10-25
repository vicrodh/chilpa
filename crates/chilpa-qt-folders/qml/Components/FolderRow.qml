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
    required property string scanPolicy

    readonly property bool scanning: LibraryFolders.scanning
        && (LibraryFolders.scanningPath === "" || LibraryFolders.scanningPath === path)

    signal rescan()
    signal forget()
    signal policyChosen(string policy)

    implicitHeight: Metrics.folderRow + checks.implicitHeight + Metrics.gap8

    Rectangle {
        anchors.fill: parent
        radius: Metrics.radiusControl
        color: hover.hovered ? Theme.color.stateHover : "transparent"
    }

    Icon {
        id: icon
        x: Metrics.gap8
        y: (Metrics.folderRow - height) / 2 // derived: centered in the folder's first line
        glyph: Icons.folder
        size: Metrics.iconMd
    }

    Column {
        anchors.left: icon.right
        anchors.leftMargin: Metrics.gap10
        anchors.right: scanned.left
        anchors.rightMargin: Metrics.gap16
        y: (Metrics.folderRow - height) / 2 // derived: centered in the folder's first line

        Text { width: parent.width; text: row.name; elide: Text.ElideRight; font.pointSize: Type.body; font.weight: Type.medium; color: Theme.color.textPrimary }
        Text { width: parent.width; text: row.path; elide: Text.ElideMiddle; font.pointSize: Type.small; color: Theme.color.textTertiary }
    }

    Text {
        id: scanned
        anchors.right: status.left
        anchors.rightMargin: Metrics.gap16
        y: (Metrics.folderRow - height) / 2 // derived: centered in the folder's first line
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
        y: (Metrics.folderRow - height) / 2 // derived: centered in the folder's first line
        width: Metrics.folderStatusWidth + Metrics.gap12
        text: row.reachable ? I18n.tr("Available", I18n.rev) : I18n.tr("Missing", I18n.rev)
        font.pointSize: Type.row
        color: row.reachable ? Theme.color.textSecondary : Theme.color.statusNegative
    }

    Row {
        id: actions
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        y: (Metrics.folderRow - height) / 2 // derived: centered in the folder's first line
        spacing: Metrics.gap4

        IconButton {
            tip: I18n.tr("Remove from the library", I18n.rev)
            glyph: Icons.trash
            enabled: !LibraryFolders.scanning
            onClicked: row.forget()
        }
    }

    Flow {
        id: checks
        x: icon.x + icon.width + Metrics.gap10
        y: Metrics.folderRow
        width: parent.width - x - Metrics.gap8
        spacing: Metrics.gap12
        Item {
            width: Math.min(label.implicitWidth, checks.width)
            height: Math.max(Metrics.fieldHeightLarge, label.implicitHeight)
            Text {
                id: label
                width: parent.width
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Check for new additions", I18n.rev)
                wrapMode: Text.WordWrap
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
        }
        SelectField {
            objectName: "folderCheckPolicy"
            width: Metrics.folderPolicyWidth
            values: ["manual", "startup", "12h", "24h", "weekly"]
            options: [I18n.tr("Manually", I18n.rev), I18n.tr("On app startup", I18n.rev),
                I18n.tr("Every 12 hours", I18n.rev), I18n.tr("Every 24 hours", I18n.rev), I18n.tr("Every week", I18n.rev)]
            value: row.scanPolicy
            onPicked: policy => row.policyChosen(policy)
        }
        PillButton {
            objectName: "checkFolderChanges"
            height: Metrics.fieldHeightLarge
            glyph: Lucide.rotateCcw
            iconFamily: Lucide.font
            text: I18n.tr("Look for changes", I18n.rev)
            enabled: !LibraryFolders.scanning && !LibraryFolders.canResume
            onClicked: row.rescan()
        }
    }

    HoverHandler { id: hover }
}
