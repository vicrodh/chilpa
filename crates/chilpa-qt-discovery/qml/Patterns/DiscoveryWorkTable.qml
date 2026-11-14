import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    id: table
    property var entries: []
    property int total: 0
    property int page: 0
    property bool loading: false
    property bool ready: true
    readonly property alias scrollView: rows
    readonly property bool counts: width >= Metrics.discoveryTableCountsMin
    readonly property real countWidth: counts ? Metrics.discoveryTableCountWidth : 0
    readonly property real nameWidth: Math.max(0, width - countWidth * 2) * Metrics.discoveryTableTitleRatio // derived: two count columns
    readonly property real composerWidth: Math.max(0, width - countWidth * 2 - nameWidth) // derived: two count columns
    signal activated(var entry)
    signal pageRequested(int page)

    Row {
        id: header
        width: parent.width; height: Metrics.rowCompact
        Repeater {
            model: [I18n.tr("Composition", I18n.rev), I18n.tr("Composer", I18n.rev), I18n.tr("Albums", I18n.rev), I18n.tr("Tracks", I18n.rev)]
            Text {
                required property string modelData
                required property int index
                width: index === 0 ? table.nameWidth : index === 1 ? table.composerWidth : table.countWidth
                visible: index < 2 || table.counts
                height: header.height; verticalAlignment: Text.AlignVCenter
                leftPadding: Metrics.gap12; rightPadding: Metrics.gap12
                text: modelData; elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.meta; font.weight: Type.semibold
            }
        }
    }
    ListView {
        id: rows
        objectName: "discoveryEntries"
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: header.bottom; anchors.bottom: paging.top
        anchors.bottomMargin: Metrics.gap12
        model: table.entries; clip: true; reuseItems: true
        cacheBuffer: Metrics.rowRich * 2
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar {}
        delegate: Button {
            id: row
            required property var modelData
            width: rows.width; height: Metrics.rowRich; padding: 0
            hoverEnabled: true
            Accessible.name: modelData.name || ""
            Accessible.description: [(modelData.composers || []).join(" · "),
                I18n.trCount("{} album", "{} albums", modelData.albums || 0, I18n.rev),
                I18n.trCount("{} track", "{} tracks", modelData.tracks || 0, I18n.rev)].join(" · ")
            onClicked: table.activated(modelData)
            Keys.onReturnPressed: clicked()
            Keys.onEnterPressed: clicked()
            background: Rectangle {
                radius: Metrics.radiusRow
                color: row.down ? Theme.color.stateSelected : row.hovered ? Theme.color.stateHover : "transparent"
                FocusRing { visible: row.activeFocus }
            }
            contentItem: Row {
                Repeater {
                    model: [row.modelData.name || "", (row.modelData.composers || []).join(" · ") || I18n.tr("Composer not tagged", I18n.rev), String(row.modelData.albums || 0), String(row.modelData.tracks || 0)]
                    Text {
                        required property string modelData
                        required property int index
                        width: index === 0 ? table.nameWidth : index === 1 ? table.composerWidth : table.countWidth
                        visible: index < 2 || table.counts
                        height: row.height; verticalAlignment: Text.AlignVCenter
                        leftPadding: Metrics.gap12; rightPadding: Metrics.gap12
                        text: modelData; textFormat: Text.PlainText; elide: Text.ElideRight
                        color: index === 0 ? Theme.color.textPrimary : Theme.color.textSecondary
                        font.pointSize: Type.body; font.weight: index === 0 ? Type.semibold : Font.Normal
                    }
                }
            }
            ToolTip.visible: hovered && (modelData.name || "").length > 0
            ToolTip.text: (modelData.name || "") + "\n" + Accessible.description
        }
    }
    Row {
        id: paging
        anchors.bottom: parent.bottom; anchors.horizontalCenter: parent.horizontalCenter
        spacing: Metrics.gap12
        visible: table.total > Metrics.discoveryPageSize
        height: visible ? implicitHeight : 0
        PillButton { text: I18n.tr("Previous", I18n.rev); enabled: table.page > 0 && !table.loading; onClicked: table.pageRequested(table.page - 1) }
        Text { anchors.verticalCenter: parent.verticalCenter; text: (table.page + 1) + " / " + Math.ceil(table.total / Metrics.discoveryPageSize); font.pointSize: Type.meta; color: Theme.color.textSecondary }
        PillButton { text: I18n.tr("Next", I18n.rev); enabled: (table.page + 1) * Metrics.discoveryPageSize < table.total && !table.loading; onClicked: table.pageRequested(table.page + 1) }
    }
}
