import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// What offline music takes, as QBZ's offline cache says it
// (OfflineManagerView, its stats bar): how many tracks, their size against
// the limit and a bar of it, and at the right the limit, the folder and
// removing everything. Without a limit there is no bar: the disk is it.
// Where there is no room for one line, the bar and the tools go under.
Rectangle {
    id: summary
    property var doc: ({})
    property bool busy: false
    signal limitPicked(string value)
    signal folderOpened()
    signal everythingRemoved()
    readonly property bool limited: (summary.doc.limitSize || "") !== ""
    readonly property bool stacked: width < stats.implicitWidth + tools.implicitWidth + Metrics.selectWidth / 2 + Metrics.gap20 * 4 // derived: the bar keeps half a select between them
    implicitHeight: (stacked ? stats.height + tools.height + Metrics.gap12 * 2 + (limited ? Metrics.offlineUsageBar + Metrics.gap12 : 0) : Math.max(stats.height, tools.height)) + Metrics.gap16 * 2 // derived: the lines, their gaps and the padding
    radius: Metrics.radiusPanel
    color: Theme.color.surfaceRaised

    Column {
        id: stats
        x: Metrics.gap16; y: Metrics.gap16
        width: summary.stacked ? summary.width - Metrics.gap16 * 2 : implicitWidth // derived: the padding on both sides
        spacing: Metrics.gap2
        Text {
            objectName: "offlineCount"
            text: I18n.trCount("{} track", "{} tracks", summary.doc.files || 0, I18n.rev)
            textFormat: Text.PlainText
            font.pointSize: Type.bodyLarge
            font.weight: Type.semibold
            color: Theme.color.textPrimary
        }
        Text {
            objectName: "offlineUsage"
            text: summary.limited ? I18n.trArgs("{} of {}", [summary.doc.size || "", summary.doc.limitSize], I18n.rev)
                : (summary.doc.size || "") + " · " + I18n.tr("Unlimited", I18n.rev)
            textFormat: Text.PlainText
            font.pointSize: Type.small
            color: summary.doc.full === true ? Theme.color.statusNegative : Theme.color.textSecondary
        }
    }
    Rectangle {
        objectName: "offlineUsageBar"
        visible: summary.limited
        x: summary.stacked ? Metrics.gap16 : stats.x + stats.width + Metrics.gap20
        y: summary.stacked ? stats.y + stats.height + Metrics.gap12 : (summary.height - height) / 2 // derived: centered
        width: summary.stacked ? summary.width - Metrics.gap16 * 2 : tools.x - x - Metrics.gap20 // derived: the padding on both sides
        height: Metrics.offlineUsageBar
        radius: height / 2 // derived: round ends
        color: Theme.color.fillField
        Rectangle {
            width: parent.width * Math.max(0, Math.min(1, summary.doc.usage || 0))
            height: parent.height
            radius: parent.radius
            color: summary.doc.full === true ? Theme.color.statusNegative : Theme.color.accent
        }
    }
    Row {
        id: tools
        x: summary.stacked ? Metrics.gap16 : summary.width - width - Metrics.gap16
        y: summary.stacked ? summary.height - height - Metrics.gap16 : (summary.height - height) / 2 // derived: centered
        spacing: Metrics.gap6
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Storage limit", I18n.rev)
            textFormat: Text.PlainText
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
        SelectField {
            id: limit
            objectName: "offlineLimit"
            anchors.verticalCenter: parent.verticalCenter
            width: limit.chrome + limit.widestOption
            options: (summary.doc.limits || {}).options || []
            values: (summary.doc.limits || {}).values || []
            value: summary.doc.limit || ""
            onPicked: value => summary.limitPicked(value)
        }
        IconButton {
            objectName: "offlineOpenFolder"
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.folder
            tip: I18n.tr("Open the offline folder", I18n.rev)
            enabled: (summary.doc.files || 0) > 0 && !!(summary.doc.root || {}).path
            onClicked: summary.folderOpened()
        }
        IconButton {
            objectName: "offlineRemoveAll"
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.trash
            tip: I18n.tr("Remove downloaded files…", I18n.rev)
            enabled: !summary.busy && (summary.doc.files || 0) > 0
            onClicked: summary.everythingRemoved()
        }
    }
}
