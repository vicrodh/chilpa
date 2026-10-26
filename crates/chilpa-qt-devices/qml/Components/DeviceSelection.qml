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
    property string kind: "all"
    property int page: 0
    property int pages: 1
    onQueryChanged: if (search && search.text !== query) search.text = query
    signal toggled(string id)
    signal filterEdited(string text)
    signal opened(string id)
    signal kindAsked(string kind)
    signal pageAsked(int page)
    radius: Metrics.radiusMenu
    color: Theme.color.surfacePane
    border.color: Theme.color.strokeHairline
    border.width: Metrics.hairline

    Row {
        id: head
        x: Metrics.gap8; y: Metrics.gap8
        width: parent.width - Metrics.gap8 * 2 // derived: equal insets
        spacing: Metrics.gap8
        SelectField {
            objectName: "deviceSelectionKind"
            width: (parent.width - Metrics.gap8) / 2 // derived: two equal fields and their gap
            options: [I18n.tr("All selections", I18n.rev), I18n.tr("Playlists", I18n.rev), I18n.tr("Queries", I18n.rev), I18n.tr("Albums and tracks", I18n.rev)]
            values: ["all", "playlists", "queries", "tracks"]
            value: panel.kind
            enabled: !panel.busy
            onPicked: kind => panel.kindAsked(kind)
        }
        SearchField {
            id: search
            objectName: "deviceSelectionSearch"
            width: (parent.width - Metrics.gap8) / 2 // derived: two equal fields and their gap
            placeholder: I18n.tr("Find music or saved selections…", I18n.rev)
            enabled: !panel.busy
            text: panel.query
            onTextChanged: if (text !== panel.query) panel.filterEdited(text)
        }
    }
    KeyedModel { id: selections; keyField: "id"; items: panel.rows }
    ListView {
        id: list
        objectName: "deviceSelections"
        x: Metrics.gap4
        y: head.y + head.height + Metrics.gap8
        width: parent.width - Metrics.gap4 * 2 - Metrics.gap16 // derived: insets and scrollbar gutter
        height: Math.max(0, footer.y - y - Metrics.gap4)
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
            readonly property var sourceEntry: model?.entry || ({})
            width: list.width
            rowIndex: index
            entry: ({id:sourceEntry.id || "",title:sourceEntry.name || "",detail:sourceEntry.description || "",checked:sourceEntry.checked === true,open:!!sourceEntry.id})
            checkable: true
            busy: panel.busy
            onToggled: id => panel.toggled(id)
            onOpened: id => panel.opened(id)
        }
    }
    DevicePager {
        id: footer
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap4
        controlPrefix: "deviceSelections"
        page: panel.page
        pages: panel.pages
        busy: panel.busy
        onPageAsked: page => panel.pageAsked(page)
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
