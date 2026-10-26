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
    property string query: ""
    onQueryChanged: if (search && search.text !== query) search.text = query
    signal toggled(string id)
    signal filterEdited(string text)
    signal opened(string id)
    radius: Metrics.radiusMenu
    color: Theme.color.surfacePane
    border.color: Theme.color.strokeHairline
    border.width: Metrics.hairline

    SearchField {
        id: search
        objectName: "deviceSelectionSearch"
        x: Metrics.gap8; y: Metrics.gap8
        width: parent.width - Metrics.gap8 * 2 // derived: equal insets
        placeholder: I18n.tr("Find music or saved selections…", I18n.rev)
        enabled: !panel.busy
        text: panel.query
        onTextChanged: if (text !== panel.query) panel.filterEdited(text)
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
        delegate: DeviceMusicRow {
            id: row
            required property var model
            required property int index
            width: list.width
            rowIndex: index
            entry: ({id:model.entry.id,title:model.entry.name,detail:model.entry.description,checked:model.entry.checked,open:true})
            checkable: true
            busy: panel.busy
            onToggled: panel.toggled(model.entry.id)
            onOpened: panel.opened(model.entry.id)
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
