import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Rectangle {
    id: panel
    property var rows: []
    property bool busy: false
    signal toggled(string id)
    signal filterEdited(string text)
    radius: Metrics.radiusMenu
    color: Theme.color.surfacePane
    border.color: Theme.color.strokeHairline
    border.width: Metrics.hairline

    SearchField {
        id: search
        objectName: "deviceSelectionSearch"
        x: Metrics.gap8; y: Metrics.gap8
        width: parent.width - Metrics.gap8 * 2 // derived: equal insets
        placeholder: I18n.tr("Find playlists or saved queries…", I18n.rev)
        enabled: !panel.busy
        onTextChanged: panel.filterEdited(text)
    }
    KeyedModel { id: selections; keyField: "id"; items: panel.rows }
    ListView {
        id: list
        objectName: "deviceSelections"
        x: Metrics.gap4
        y: search.y + search.height + Metrics.gap8
        width: parent.width - Metrics.gap4 * 2 - Metrics.gap16 // derived: insets and scrollbar gutter
        height: Math.max(0, parent.height - y - Metrics.gap4)
        model: selections
        clip: true
        reuseItems: true
        cacheBuffer: DeviceLayout.rowHeight * 2 // derived: two rows on each side
        boundsBehavior: Flickable.StopAtBounds
        Controls.ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.gap4 }
        delegate: Item {
            id: row
            required property var entry
            required property int index
            width: list.width
            height: DeviceLayout.rowHeight
            ListRowSurface { anchors.fill: parent; rowIndex: row.index; selected: row.entry.checked; hovered: hover.hovered }
            HoverHandler { id: hover }
            CircleCheck {
                objectName: "deviceSelectionCheck"
                x: Metrics.gap4
                width: parent.width - Metrics.gap8
                height: Metrics.button32
                text: row.entry.name
                checked: row.entry.checked
                enabled: !panel.busy
                onClicked: panel.toggled(row.entry.id)
            }
            Text {
                x: Metrics.gap8 + Metrics.iconMd + Metrics.gap8
                y: Metrics.button32
                width: parent.width - x - Metrics.gap8
                text: row.entry.description
                textFormat: Text.PlainText
                elide: Text.ElideRight
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
        }
    }
    Text {
        anchors.centerIn: list
        width: list.width - Metrics.gap16 * 2 // derived: symmetric empty-status padding
        visible: panel.rows.length === 0 && !panel.busy
        text: I18n.tr("No matching selections. Create a local playlist or save a query in Refine.", I18n.rev)
        wrapMode: Text.WordWrap
        horizontalAlignment: Text.AlignHCenter
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }
    CutTextTips { watching: list }
}
