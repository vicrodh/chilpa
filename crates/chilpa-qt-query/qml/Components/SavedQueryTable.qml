import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Primitives"
import "../Theme"

// I keep catalog rows in one bounded viewport, with fixed columns and row height.
Rectangle {
    id: table
    property var entries: []
    property bool readFailed: false
    property bool editable: true
    signal openAsked(string id)
    signal renameAsked(var entry)
    signal removeAsked(var entry)
    readonly property real nameWidth: (list.width - Refine.savedActionsWide - Refine.innerApart) * Refine.savedNameShare
    radius: Refine.groupRound
    color: Theme.color.surfacePane
    border.width: Metrics.hairline
    border.color: Theme.color.strokeHairline

    Item {
        id: header
        x: Refine.pickPad
        width: list.width
        height: Refine.savedHeaderTall
        Text {
            anchors.verticalCenter: parent.verticalCenter
            x: Refine.rowPadLeft
            text: I18n.tr("Name", I18n.rev)
            font.pointSize: Type.label
            font.weight: Type.semibold
            font.letterSpacing: Type.labelTracking
            color: Theme.color.textSecondary
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            x: table.nameWidth + Refine.innerApart
            text: I18n.tr("Filter", I18n.rev)
            font.pointSize: Type.label
            font.weight: Type.semibold
            font.letterSpacing: Type.labelTracking
            color: Theme.color.textSecondary
        }
    }
    Rectangle { x: Refine.pickPad; y: header.height; width: list.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
    KeyedModel { id: catalog; keyField: "id"; items: table.entries }
    ListView {
        id: list
        objectName: "savedQueriesList"
        x: Refine.pickPad
        y: header.height + Refine.rowsBottom
        width: parent.width - Refine.pickPad * 2 - Metrics.gap16 // derived: the scrollbar has its own gutter
        height: Math.max(0, parent.height - y - Refine.pickPad)
        model: catalog
        clip: true
        reuseItems: true
        boundsBehavior: Flickable.StopAtBounds
        cacheBuffer: Refine.savedRowTall * 2 // derived: two rows on each side
        Controls.ScrollBar.vertical: ViewportScrollBar { objectName: "savedQueriesScrollBar"; gutter: Refine.pickPad }
        delegate: Item {
            id: row
            required property var entry
            required property int index
            width: list.width
            height: Refine.savedRowTall
            ListRowSurface { anchors.fill: parent; radius: Refine.rowRound; rowIndex: row.index; hovered: hover.hovered }
            HoverHandler { id: hover }
            Text {
                objectName: "savedQueryRowName"
                x: Refine.rowPadLeft
                width: table.nameWidth - Refine.rowPadLeft - Refine.innerApart
                anchors.verticalCenter: parent.verticalCenter
                text: row.entry.name
                textFormat: Text.PlainText
                elide: Text.ElideRight
                font.pointSize: Type.body
                font.weight: Type.medium
                color: Theme.color.textPrimary
            }
            Text {
                objectName: "savedQueryRowFilter"
                x: table.nameWidth + Refine.innerApart
                width: operations.x - x - Refine.headApart
                anchors.verticalCenter: parent.verticalCenter
                text: row.entry.expression
                textFormat: Text.PlainText
                elide: Text.ElideRight
                font.family: Type.consoleFamily
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
            Row {
                id: operations
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: Refine.innerApart
                enabled: table.editable
                opacity: enabled ? 1 : Metrics.disabledOpacity
                SquareButton { objectName: "savedQueryOpen"; small: true; glyph: Lucide.fileUp; iconFamily: Lucide.font; tip: I18n.tr("Open", I18n.rev) + ": " + row.entry.name; onClicked: table.openAsked(row.entry.id) }
                SquareButton { objectName: "savedQueryRename"; small: true; glyph: Icons.pencil; tip: I18n.tr("Rename", I18n.rev) + ": " + row.entry.name; onClicked: table.renameAsked(row.entry) }
                SquareButton { objectName: "savedQueryRemove"; small: true; glyph: Icons.trash; tip: I18n.tr("Delete", I18n.rev) + ": " + row.entry.name; onClicked: table.removeAsked(row.entry) }
            }
        }
    }
    CutTextTips { watching: list }
    Text {
        objectName: "savedQueriesEmpty"
        visible: !table.readFailed && table.entries.length === 0
        anchors.centerIn: list
        width: list.width - Refine.pickPad * 2 // derived: equal insets for the empty state
        text: I18n.tr("No saved queries yet. Add conditions, then choose Save query.", I18n.rev)
        wrapMode: Text.WordWrap
        horizontalAlignment: Text.AlignHCenter
        font.pointSize: Type.body
        color: Theme.color.textTertiary
    }
}
