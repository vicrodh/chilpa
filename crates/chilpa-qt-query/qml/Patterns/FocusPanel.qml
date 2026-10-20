import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa
import app.chilpa.kit
import app.chilpa.query
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

Rectangle {
    id: panel
    objectName: "focusPanel"
    color: Theme.ambient ? Theme.color.ambientRaised : Theme.color.surfacePane
    radius: Metrics.radiusMenu
    property bool advancedOpen: false
    property string category: ""
    readonly property var parsed: {
        I18n.rev
        return JSON.parse(QueryChips.focusRows(Library.queryText))
    }
    readonly property var categories: [
        { field: "genre", text: I18n.tr("Genre", I18n.rev) },
        { field: "year", text: I18n.tr("Year", I18n.rev) },
        { field: "artist", text: I18n.tr("Artist", I18n.rev) },
        { field: "album", text: I18n.tr("Album", I18n.rev) },
        { field: "composer", text: I18n.tr("Composer", I18n.rev) },
        { field: "label", text: I18n.tr("Label", I18n.rev) },
        { field: "rating", text: I18n.tr("Rating", I18n.rev) },
        { field: "added", text: I18n.tr("Added", I18n.rev) },
        { field: "genre", missing: true, text: I18n.tr("No genre", I18n.rev) },
        { field: "composer", missing: true, text: I18n.tr("No composer", I18n.rev) },
        { field: "label", missing: true, text: I18n.tr("No label", I18n.rev) }
    ]
    function edit(change) { Library.applyQuery(QueryChips.edit(Library.queryText, JSON.stringify(change))) }
    function choose(index) {
        const item = categories[index]
        if (item.missing) edit({ kind: "missing", field: item.field, on: true })
        else if (item.field === "added") edit({ kind: "within", field: "added", days: 30 })
        else if (item.field === "rating") edit({ kind: "minimum", field: "rating", stars: 3 })
        else category = item.field
    }
    onVisibleChanged: if (!visible) { category = ""; menu.close() }
    onParsedChanged: if (parsed.advanced) category = ""
    Row {
        id: toolbar
        x: Metrics.gap12
        y: Metrics.gap8
        spacing: Metrics.gap8
        PillButton {
            objectName: "focusAdd"
            text: I18n.tr("Add filter", I18n.rev)
            glyph: Icons.plus
            enabled: !panel.parsed.advanced
            onClicked: menu.open()
            Menu {
                id: menu
                objectName: "focusCategories"
                y: parent.height
                entries: panel.categories
                onChosen: (index) => panel.choose(index)
            }
        }
        PillButton {
            objectName: "focusAdvanced"
            text: I18n.tr("Advanced", I18n.rev)
            glyph: panel.advancedOpen ? Icons.chevronUp : Icons.chevronDown
            kind: "ghost"
            onClicked: panel.advancedOpen = !panel.advancedOpen
        }
        SegmentedControl {
            objectName: "focusJoin"
            visible: panel.advancedOpen
            current: panel.parsed.any ? 1 : 0
            options: [{ text: I18n.tr("Match all", I18n.rev) }, { text: I18n.tr("Match any", I18n.rev) }]
            onActivated: (index) => Library.applyQuery(QueryChips.joined(Library.queryText, index === 1))
        }
    }
    Controls.ScrollView {
        id: rows
        anchors.top: toolbar.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: picker.left
        anchors.margins: Metrics.gap12
        clip: true
        contentWidth: availableWidth
        Column {
            width: rows.availableWidth
            spacing: Metrics.gap12
            Repeater {
                model: panel.parsed.rows
                FocusCondition {
                    required property var modelData
                    objectName: "focusCondition"
                    row: modelData
                    any: panel.parsed.any
                    onEdited: (change) => panel.edit(change)
                }
            }
            PillButton {
                objectName: "focusAdvancedQuery"
                visible: panel.parsed.advanced
                text: panel.parsed.text
                onClicked: TipState.show(I18n.tr("Edit with :filter", I18n.rev), this)
                Component.onDestruction: TipState.hide(this)
            }
            Text {
                visible: panel.parsed.advanced
                text: I18n.tr("Edit with :filter", I18n.rev)
                color: Theme.color.textSecondary
                font.pointSize: Type.meta
            }
        }
    }
    Item {
        id: picker
        anchors.right: parent.right
        anchors.top: toolbar.bottom
        anchors.bottom: parent.bottom
        anchors.margins: Metrics.gap12
        width: panel.category ? Math.min(Metrics.menuMaxWidth, panel.width / 2) : 0 // derived: at most half the panel
        visible: panel.category !== ""
        Row {
            id: pickerHead
            width: parent.width
            Text {
                width: parent.width - close.implicitWidth
                anchors.verticalCenter: parent.verticalCenter
                text: (panel.categories.find(c => c.field === panel.category) || {}).text || ""
                font.pointSize: Type.label
                color: Theme.color.textSecondary
            }
            IconButton { id: close; objectName: "focusCloseValues"; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: panel.category = "" }
        }
        Loader {
            objectName: "focusPicker"
            anchors.top: pickerHead.bottom
            anchors.bottom: parent.bottom
            width: parent.width
            active: panel.visible && panel.category !== ""
            sourceComponent: panel.category === "year" ? yearPicker : valuePicker
        }
    }
    Component {
        id: valuePicker
        FocusValues {
            field: panel.category
            onPicked: (value, excluded) => panel.edit({ kind: "value", field: field, value: value, state: excluded ? "exclude" : "include" })
        }
    }
    Component {
        id: yearPicker
        FocusYears {
            onEdited: (from, to) => {
                panel.edit({ kind: "range", field: "year", from: from, to: to })
                panel.category = ""
            }
        }
    }
}
