import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Controls.Popup {
    id: popup
    PopupInput { surface: popup }
    property var model
    property string field: ""
    property string label: ""
    property var draft: []
    property var options: []
    // One page of 100 entries and at most 64 back cursors live until close.
    property var cursors: [""]
    property string requestedCursor: ""
    property string next: ""
    property bool more: false
    property string error: ""
    property string searched: ""
    signal applied(string field, var values)
    parent: Controls.Overlay.overlay
    modal: true
    dim: false
    focus: true
    padding: Metrics.gap12
    margins: Metrics.gap10
    width: Math.min(Metrics.menuMaxWidth, parent ? parent.width - margins * 2 : Metrics.menuWidth) // derived: a margin on each side
    closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnReleaseOutside
    function fit() {
        if (!parent) return
        x = Math.max(margins, Math.min(x, parent.width - width - margins))
        y = Math.max(margins, Math.min(y, parent.height - height - margins))
    }
    onHeightChanged: if (opened) fit()
    function show(field, label, selected, anchor) {
        popup.field = field
        popup.label = label
        draft = Array.isArray(selected) ? selected.slice() : typeof selected === "boolean" ? [String(selected)] : []
        search.text = ""
        searched = ""
        options = []
        error = ""
        more = false
        cursors = [""]
        const at = anchor.mapToItem(parent, 0, anchor.height)
        x = Math.max(margins, Math.min(at.x, parent.width - width - margins))
        open()
        y = Math.max(margins, Math.min(at.y, parent.height - height - margins))
        ask("")
        search.focusInput()
    }
    function ask(cursor) {
        requestedCursor = cursor
        if (opened && model) model.requestColumn(field, searched, cursor)
    }
    function toggle(key) {
        if (draft.indexOf(key) >= 0) draft = draft.filter(value => value !== key)
        else draft = field === "favorite" ? [key] : draft.concat([key])
    }
    function apply() {
        applied(field, field === "favorite" ? (draft.length ? draft[0] === "true" : null) : draft)
        close()
    }
    function display(key, label) {
        if (field === "favorite") return key === "true" ? I18n.tr("Yes", I18n.rev) : I18n.tr("No", I18n.rev)
        if (field === "rating") return key === "0" ? I18n.tr("Unrated", I18n.rev) : key + " / 5"
        if (field === "quality") return ({lossy: I18n.tr("Lossy", I18n.rev), lossless: I18n.tr("Lossless", I18n.rev), hires: I18n.tr("Hi-Res", I18n.rev), dsd: "DSD"})[key] || I18n.tr("Unknown", I18n.rev)
        if (field === "format") return key ? key.toUpperCase() : I18n.tr("Unknown", I18n.rev)
        return label || I18n.tr("Unknown", I18n.rev)
    }
    function refresh() {
        if (!opened) return
        options = []; more = false; cursors = [""]; ask("")
    }
    Connections {
        target: popup.model
        ignoreUnknownSignals: true
        function onKeyChanged() { popup.refresh() }
        function onFilterRevChanged() { popup.refresh() }
        function onColumnResultChanged() {
            if (!popup.opened) return
            const result = JSON.parse(popup.model.columnResult || "{}")
            if (result.field !== popup.field || result.search !== popup.searched || result.cursor !== popup.requestedCursor) return
            popup.options = result.items || []
            popup.next = result.next || ""
            popup.more = result.more === true
            popup.error = result.error || ""
        }
    }
    Timer {
        id: settle
        interval: Motion.searchSettle
        onTriggered: {
            const next = search.text.trim()
            if (next === popup.searched) return
            popup.searched = next; popup.options = []; popup.more = false; popup.cursors = [""]; popup.ask("")
        }
    }
    onClosed: { settle.stop(); if (model) model.closeColumn(); options = []; draft = []; cursors = [""]; error = "" }
    background: Rectangle { radius: Metrics.radiusMenu; color: Theme.color.surfaceRaised; Edge {} }
    contentItem: Column {
        id: contents
        readonly property real fixedHeight: heading.height + search.height + allRow.height + footer.height
            + (pager.visible ? pager.height + spacing : 0) + (status.visible ? status.height + spacing : 0) + spacing * 4 // derived: gaps between the four fixed controls and the list
        spacing: Metrics.gap8
        Text {
            id: heading
            width: parent.width
            text: popup.label
            font.pointSize: Type.body
            font.weight: Type.semibold
            color: Theme.color.textPrimary
        }
        SearchField {
            id: search
            objectName: "tracksFilterSearch"
            width: parent.width
            placeholder: I18n.tr("Search", I18n.rev)
            onTextChanged: if (popup.opened) settle.restart()
            onEscaped: popup.close()
            onAccepted: popup.apply()
            onStepped: by => { choices.forceActiveFocus(); choices.currentIndex = Math.max(0, Math.min(choices.count - 1, choices.currentIndex + by)); choices.positionViewAtIndex(choices.currentIndex, ListView.Contain) }
        }
        MenuEntry {
            id: allRow
            objectName: "tracksFilterAll"
            width: parent.width
            entry: ({text: I18n.tr("All", I18n.rev), checked: popup.draft.length === 0})
            onClicked: popup.draft = []
        }
        ListView {
            id: choices
            objectName: "tracksFilterChoices"
            width: parent.width
            height: Math.min(contentHeight, Math.max(0, (popup.parent ? popup.parent.height : Metrics.menuMaxWidth) - popup.padding * 2 - popup.margins * 2 - contents.fixedHeight)) // derived: panel padding and outer margins on both sides
            clip: true
            reuseItems: true
            activeFocusOnTab: true
            Keys.onReturnPressed: popup.apply()
            Keys.onEnterPressed: popup.apply()
            model: popup.options
            boundsBehavior: Flickable.StopAtBounds
            Controls.ScrollBar.vertical: Controls.ScrollBar {}
            Keys.onSpacePressed: if (currentIndex >= 0 && currentIndex < popup.options.length) popup.toggle(popup.options[currentIndex].key)
            delegate: MenuEntry {
                required property var modelData
                required property int index
                width: choices.width
                entry: ({text: popup.display(modelData.key, modelData.label), hint: String(modelData.count), checked: popup.draft.indexOf(modelData.key) >= 0, glyph: popup.field === "rating" && modelData.key !== "0" ? Icons.starFilled : "", filled: popup.field === "rating", focused: choices.activeFocus && index === choices.currentIndex})
                onClicked: popup.toggle(modelData.key)
            }
        }
        Text {
            id: status
            width: parent.width
            visible: popup.error !== "" || popup.options.length === 0
            text: popup.error || (popup.model && popup.model.columnBusy ? I18n.tr("Loading…", I18n.rev) : I18n.tr("No matches", I18n.rev))
            wrapMode: Text.WordWrap
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
        Row {
            id: pager
            visible: popup.more || popup.cursors.length > 1
            spacing: Metrics.gap8
            PillButton {
                objectName: "tracksFilterPrevious"
                visible: popup.cursors.length > 1
                enabled: popup.model && !popup.model.columnBusy
                text: I18n.tr("Previous", I18n.rev)
                onClicked: { popup.cursors = popup.cursors.slice(0, -1); popup.ask(popup.cursors[popup.cursors.length - 1]) }
            }
            PillButton {
                objectName: "tracksFilterMore"
                visible: popup.more
                enabled: popup.model && !popup.model.columnBusy
                text: I18n.tr("Next", I18n.rev)
                onClicked: { popup.cursors = popup.cursors.concat([popup.next]).slice(-64); popup.ask(popup.next) }
            }
        }
        Row {
            id: footer
            anchors.right: parent.right
            spacing: Metrics.gap8
            PillButton { text: I18n.tr("Cancel", I18n.rev); onClicked: popup.close() }
            PillButton { objectName: "tracksFilterApply"; kind: "primary"; text: I18n.tr("Apply", I18n.rev); onClicked: popup.apply() }
        }
    }
}
