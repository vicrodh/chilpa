import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Rectangle {
    id: table
    property var rows: []
    property int page: 0
    property int pages: 1
    property bool busy: false
    property string emptyText: I18n.tr("No files need to change.", I18n.rev)
    signal pageAsked(int page)
    radius: Metrics.radiusMenu
    color: Theme.color.surfacePane
    border.color: Theme.color.strokeHairline
    border.width: Metrics.hairline

    Row {
        id: header
        x: Metrics.gap12
        width: list.width - Metrics.gap8 // derived: the same content edge as the rows
        height: DeviceLayout.tableHeader
        Text { width: DeviceLayout.actionWidth - Metrics.gap8; height: parent.height; verticalAlignment: Text.AlignVCenter; text: I18n.tr("Action", I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.label }
        Text { width: Math.max(0, parent.width - DeviceLayout.actionWidth - DeviceLayout.sizeWidth + Metrics.gap8); height: parent.height; verticalAlignment: Text.AlignVCenter; text: I18n.tr("Music", I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.label }
        Text { width: DeviceLayout.sizeWidth; height: parent.height; verticalAlignment: Text.AlignVCenter; horizontalAlignment: Text.AlignRight; text: I18n.tr("Size", I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.label }
    }
    Rectangle { x: Metrics.gap4; y: header.height; width: parent.width - Metrics.gap4 - Metrics.gap4; height: Metrics.hairline; color: Theme.color.strokeHairline }
    KeyedModel { id: changes; keyField: "path"; items: table.rows }
    onPageChanged: list.positionViewAtBeginning()
    ListView {
        id: list
        objectName: "deviceChangeList"
        x: Metrics.gap4
        y: header.height + Metrics.hairline
        width: parent.width - Metrics.gap4 * 2 - Metrics.gap16 // derived: insets and scrollbar gutter
        height: Math.max(0, footer.y - y - Metrics.gap4)
        clip: true
        model: changes
        reuseItems: true
        cacheBuffer: DeviceLayout.rowHeight * 2 // derived: two adjacent rows
        boundsBehavior: Flickable.StopAtBounds
        Controls.ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.gap4 }
        delegate: Item {
            id: row
            required property var entry
            required property int index
            width: list.width
            height: DeviceLayout.rowHeight
            ListRowSurface { anchors.fill: parent; rowIndex: row.index; hovered: hover.hovered }
            HoverHandler { id: hover }
            Text {
                x: Metrics.gap8
                width: DeviceLayout.actionWidth - Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                text: row.entry.action
                color: Theme.color.textSecondary
                font.pointSize: Type.body
                elide: Text.ElideRight
            }
            Column {
                x: DeviceLayout.actionWidth
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, parent.width - x - DeviceLayout.sizeWidth)
                spacing: Metrics.gap4
                Text { width: parent.width; text: row.entry.title; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.body; font.weight: Type.medium }
                Text { width: parent.width; text: row.entry.playlist ? I18n.tr("Playlist", I18n.rev) : [row.entry.artist, row.entry.album].filter(Boolean).join(" · "); textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.small }
            }
            Text {
                width: DeviceLayout.sizeWidth - Metrics.gap8
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                horizontalAlignment: Text.AlignRight
                text: row.entry.size
                color: Theme.color.textSecondary
                font.pointSize: Type.small
                font.features: { "tnum": 1 }
            }
        }
    }
    Row {
        id: footer
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap4
        spacing: Metrics.gap8
        IconButton { objectName: "devicePreviousPage"; glyph: Icons.chevronLeft; tip: I18n.tr("Previous page", I18n.rev); enabled: table.page > 0 && !table.busy; onClicked: table.pageAsked(table.page - 1) }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.trArgs("Page {} of {}", [String(table.page + 1), String(table.pages)], I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.small }
        IconButton { objectName: "deviceNextPage"; glyph: Icons.chevronRight; tip: I18n.tr("Next page", I18n.rev); enabled: table.page + 1 < table.pages && !table.busy; onClicked: table.pageAsked(table.page + 1) }
    }
    Text {
        anchors.centerIn: list
        width: list.width - Metrics.gap16
        visible: table.rows.length === 0
        text: table.emptyText
        wrapMode: Text.WordWrap
        horizontalAlignment: Text.AlignHCenter
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }
    CutTextTips { watching: list }
}
