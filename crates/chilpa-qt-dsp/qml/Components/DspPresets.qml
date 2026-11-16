import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: browser
    objectName: "dspPresetBrowser"
    property alias deletionDialog: removal
    property var catalog: ({})
    readonly property string rowsData: JSON.stringify(catalog.presets || [])
    property var presets: JSON.parse(rowsData)
    property var groups: catalog.groups || []
    property string group: "all"
    property string query: ""
    property string selectedKey: ""
    property string selectedDescription: ""
    property var selectedIds: catalog.selected_ids || []
    property bool selectedPersonal: !!catalog.selected_personal
    property bool loading: !!catalog.loading
    property int total: catalog.total || 0
    property int offset: catalog.offset || 0
    property var nextOffset: catalog.next === undefined ? null : catalog.next
    signal groupPicked(string value)
    signal queryEdited(string value)
    signal searchRequested(int offset)
    signal chosen(var entry)
    signal checked(string id, bool selected)
    signal copyRequested(var ids)
    signal removeRequested(var ids)
    spacing: Metrics.gap12
    onQueryChanged: searchDelay.restart()
    Timer { id: searchDelay; interval: Metrics.dspSearchInterval; onTriggered: browser.searchRequested(0) }
    Row {
        id: filters
        width: parent.width
        spacing: Metrics.gap12
        SelectField {
            width: (parent.width - parent.spacing) / 2 // derived: equal filter and search columns
            options: browser.groups.map(entry => entry.label)
            values: browser.groups.map(entry => entry.key)
            value: browser.group
            onPicked: value => browser.groupPicked(value)
        }
        SearchField {
            objectName: "dspSearch"
            width: (parent.width - parent.spacing) / 2 // derived: equal filter and search columns
            text: browser.query
            placeholder: I18n.tr("Find a preset or device", I18n.rev)
            onTextChanged: if (text !== browser.query) browser.queryEdited(text)
        }
    }
    Text {
        id: description
        width: parent.width
        visible: text !== ""
        text: browser.selectedDescription
        maximumLineCount: 2
        elide: Text.ElideRight
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
    ListView {
        id: results
        objectName: "dspPresetResults"
        width: parent.width
        height: Math.max(0, browser.height - filters.height - actions.height - navigation.height - browser.spacing * 3 - (description.visible ? description.height + browser.spacing : 0)) // derived: list fills the space between fixed search and actions
        model: browser.presets
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        Controls.ScrollBar.vertical: ViewportScrollBar {}
        delegate: Rectangle {
            id: row
            required property var modelData
            width: results.width
            height: Metrics.dspPresetRowHeight
            readonly property bool selected: browser.selectedKey === modelData.plugin + "/" + modelData.preset.id
            color: selected ? Theme.color.stateSelected : pointer.containsMouse ? Theme.color.stateHover : "transparent"
            radius: Metrics.radiusControl
            Toggle {
                id: pick
                compact: true
                anchors.left: parent.left
                anchors.leftMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                checked: browser.selectedIds.includes(row.modelData.preset.id)
                Accessible.name: I18n.tr("Select preset", I18n.rev) + " " + row.modelData.preset.name
                onToggled: on => browser.checked(row.modelData.preset.id, on)
            }
            Item {
                anchors.left: pick.right
                anchors.right: marker.left
                anchors.margins: Metrics.gap12
                height: parent.height
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width
                    spacing: Metrics.gap6
                    Text { width: parent.width; text: row.modelData.preset.name; elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.bodyLarge; font.weight: row.selected ? Type.semibold : Type.medium }
                    Text { width: parent.width; text: row.modelData.preset.source || row.modelData.preset.category; elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.small }
                }
                activeFocusOnTab: true
                Accessible.name: row.modelData.preset.name
                Accessible.role: Accessible.Button
                Keys.onReturnPressed: browser.chosen(row.modelData)
                Keys.onSpacePressed: browser.chosen(row.modelData)
                FocusRing {}
                MouseArea { id: pointer; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: browser.chosen(row.modelData) }
            }
            Icon { id: marker; anchors.right: parent.right; anchors.rightMargin: Metrics.gap16; anchors.verticalCenter: parent.verticalCenter; glyph: row.selected ? Icons.check : Icons.chevronRight; color: row.selected ? Theme.color.accent : Theme.color.textTertiary }
        }
        Text { anchors.centerIn: parent; width: parent.width; visible: results.count === 0; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; text: browser.loading ? I18n.tr("Searching presets…", I18n.rev) : I18n.tr("No presets match this search.", I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.body }
    }
    Row {
        id: actions
        width: parent.width
        spacing: Metrics.gap8
        PillButton { text: I18n.tr("Copy selected JSON", I18n.rev); enabled: browser.selectedIds.length > 0; onClicked: browser.copyRequested(browser.selectedIds) }
        PillButton { objectName: "dspDeleteSelected"; text: I18n.tr("Delete selected", I18n.rev); enabled: browser.selectedPersonal && browser.selectedIds.length > 0; onClicked: { removal.ids = browser.selectedIds.slice(); removal.open() } }
        Text { anchors.verticalCenter: parent.verticalCenter; text: browser.selectedIds.length + " / 64"; color: Theme.color.textTertiary; font.pointSize: Type.small }
    }
    Row {
        id: navigation
        width: parent.width
        spacing: Metrics.gap12
        PillButton { text: I18n.tr("Previous", I18n.rev); enabled: browser.offset > 0 && !browser.loading; onClicked: browser.searchRequested(Math.max(0, browser.offset - 32)) }
        PillButton { text: I18n.tr("Next", I18n.rev); enabled: browser.nextOffset !== null && !browser.loading; onClicked: browser.searchRequested(Number(browser.nextOffset)) }
        Text { anchors.verticalCenter: parent.verticalCenter; text: browser.loading ? I18n.tr("Searching presets…", I18n.rev) : browser.total + " " + I18n.tr("presets", I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.small }
    }
    ConfirmationDialog {
        id: removal
        property var ids: []
        title: I18n.tr("Delete selected presets?", I18n.rev)
        message: I18n.tr("This permanently deletes the selected personal presets and clears their assignments.", I18n.rev)
        confirmText: I18n.tr("Delete selected", I18n.rev)
        destructive: true
        targetValid: JSON.stringify(ids) === JSON.stringify(browser.selectedIds)
        onConfirmed: browser.removeRequested(ids)
    }
}
