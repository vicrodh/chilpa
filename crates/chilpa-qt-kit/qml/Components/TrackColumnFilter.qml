import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "../Theme"
import "../Primitives"

Controls.Popup {
    id: popup
    objectName: "tracksColumnFilter"
    PopupInput { surface: popup }
    property var model
    property string field: ""
    property string label: ""
    property var draft: []
    property string searched: ""
    property var sourceNames: ({})
    // Selected source labels only; no title, album or artist catalog is cached.
    property var labels: ({})
    property Item anchorItem: null
    property Item contentViewport: null
    property real anchorTop: margins
    property real anchorBottom: margins
    property real anchorLeft: margins
    readonly property bool below: !parent || anchorBottom + height + margins <= parent.height
    readonly property real panelRoom: Math.max(0, Math.min((contentViewport ? contentViewport.height : parent ? parent.height : Metrics.trackFilterHeight) * Metrics.trackFilterHeightShare,
        parent ? parent.height - margins * 2 : Metrics.trackFilterHeight)) // derived: the whole flyout is at most 40% of the content
    readonly property bool compact: panelRoom < Metrics.trackFilterShortHeight
    readonly property alias choiceWindow: optionsWindow
    signal applied(string field, var values, var labels)
    parent: Controls.Overlay.overlay
    modal: true
    dim: false
    focus: true
    padding: compact ? Metrics.gap8 : Metrics.gap12
    margins: Metrics.gap10
    height: Math.min(panelRoom, contents.implicitHeight + topPadding + bottomPadding)
    width: Math.min(Metrics.trackFilterWidth, parent ? parent.width - margins * 2 : Metrics.menuWidth) // derived: outer margins
    closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnReleaseOutside
    function fit() {
        if (!parent) return
        if (anchorItem) {
            const at = anchorItem.mapToItem(parent, 0, 0)
            anchorTop = at.y; anchorBottom = at.y + anchorItem.height; anchorLeft = at.x
        }
        x = Math.max(margins, Math.min(anchorLeft, parent.width - width - margins))
        const wanted = below ? anchorBottom : height + margins <= anchorTop ? anchorTop - height : anchorBottom
        y = Math.max(margins, Math.min(wanted, parent.height - height - margins))
    }
    onHeightChanged: if (opened) fit()
    Connections {
        target: popup.parent
        function onWidthChanged() { if (popup.opened) popup.fit() }
        function onHeightChanged() { if (popup.opened) popup.fit() }
    }
    function show(column, title, selected, anchor) {
        field = column; label = title
        draft = Array.isArray(selected) ? selected.slice() : typeof selected === "boolean" ? [String(selected)] : []
        labels = names.selectedSources(sourceNames, draft)
        search.text = ""; searched = ""
        anchorItem = anchor
        fit(); open(); fit()
        optionsWindow.reset(field, "")
        search.focusInput()
    }
    function toggle(key) {
        if (draft.indexOf(key) >= 0) draft = draft.filter(value => value !== key)
        else draft = field === "favorite" ? [key] : draft.concat([key])
        if (field === "source") {
            const next = Object.assign({}, labels)
            if (draft.indexOf(key) < 0) delete next[key]
            else if (Object.keys(next).length < 256) {
                for (let i = 0; i < optionsWindow.count; ++i) {
                    const option = optionsWindow.optionAt(i)
                    if (option.key === key) { next[key] = option.label; break }
                }
            }
            labels = next
        }
    }
    function apply() {
        const column = field, selected = field === "favorite" ? (draft.length ? draft[0] === "true" : null) : draft.slice()
        const names = labels
        close()
        applied(column, selected, names)
    }
    function display(key, label) { return names.value(field, key, label) }
    TrackFilterNames { id: names; sourceNames: popup.labels }
    TrackChoiceWindow {
        id: optionsWindow
        model: popup.model
        onAboutToChange: choices.capture()
        onShifted: rows => choices.shift += rows
        onPageArrived: items => {
            if (popup.field === "source") popup.labels = names.rememberSources(popup.labels, popup.draft, items)
            choices.restore()
        }
    }
    Connections {
        target: popup.model
        ignoreUnknownSignals: true
        function onKeyChanged() { if (popup.opened) optionsWindow.reset(popup.field, popup.searched) }
        function onFilterRevChanged() { if (popup.opened) optionsWindow.reset(popup.field, popup.searched) }
    }
    Timer {
        id: settle
        interval: Motion.searchSettle
        onTriggered: {
            const next = search.text.trim()
            if (next === popup.searched) return
            popup.searched = next
            optionsWindow.reset(popup.field, next)
            choices.positionViewAtBeginning()
        }
    }
    onClosed: { settle.stop(); optionsWindow.close(); draft = []; labels = ({}); anchorItem = null }
    background: Rectangle { radius: Metrics.radiusMenu; color: Theme.color.surfaceRaised; Edge {} }
    contentItem: Column {
        id: contents
        readonly property real fixedHeight: (heading.visible ? heading.height : 0) + search.height + allRow.height + footer.height
            + (status.visible ? status.height + spacing : 0) + spacing * (heading.visible ? 4 : 3) // derived: gaps between the visible fixed controls and list
        spacing: popup.compact ? Metrics.gap4 : Metrics.gap8
        clip: true
        Text {
            id: heading
            visible: !popup.compact
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
            onStepped: by => { choices.forceActiveFocus(); choices.currentIndex = Math.max(0, Math.min(choices.count - 1, choices.currentIndex + by)); choices.positionViewAtIndex(choices.currentIndex, ListView.Contain); choices.prefetch(by) }
        }
        MenuEntry {
            id: allRow
            objectName: "tracksFilterAll"
            width: parent.width
            entry: ({text: I18n.tr("All", I18n.rev), checked: popup.draft.length === 0})
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: entry.text
            Accessible.onPressAction: popup.draft = []
            Keys.onReturnPressed: popup.draft = []
            Keys.onSpacePressed: popup.draft = []
            onClicked: popup.draft = []
            FocusRing {}
        }
        TrackFilterChoices {
            id: choices
            width: parent.width
            height: Math.min(contentHeight, Math.max(0, popup.panelRoom - popup.padding * 2 - contents.fixedHeight)) // derived: panel padding
            choiceWindow: popup.choiceWindow
            surfaceOpen: popup.opened
            field: popup.field
            draft: popup.draft
            formatter: names
            onToggled: key => popup.toggle(key)
            onAccepted: popup.apply()
        }
        Text {
            id: status
            width: parent.width
            visible: optionsWindow.error !== "" || optionsWindow.count === 0
            text: optionsWindow.error || (optionsWindow.loading ? I18n.tr("Loading…", I18n.rev) : I18n.tr("No matches", I18n.rev))
            wrapMode: Text.WordWrap
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
        Item {
            id: footer
            width: parent.width
            height: Metrics.hitMin
            Rectangle { y: -Metrics.gap4; width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
            Text {
                anchors.left: parent.left
                anchors.right: actions.left
                anchors.rightMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                text: optionsWindow.loading && optionsWindow.count > 0 ? I18n.tr("Loading…", I18n.rev) : popup.draft.length ? I18n.trCount("{} selection", "{} selections", popup.draft.length, I18n.rev) : I18n.tr("All", I18n.rev)
                elide: Text.ElideRight
                font.pointSize: Type.small
                color: Theme.color.textTertiary
            }
            Row {
                id: actions
                anchors.right: parent.right
                spacing: Metrics.gap6
                CompactButton { visible: optionsWindow.error !== ""; text: I18n.tr("Retry", I18n.rev); onClicked: optionsWindow.retry() }
                CompactButton { objectName: "tracksFilterCancel"; text: I18n.tr("Cancel", I18n.rev); quiet: true; onClicked: popup.close() }
                CompactButton { objectName: "tracksFilterApply"; text: I18n.tr("Apply", I18n.rev); primary: true; onClicked: popup.apply() }
            }
        }
    }
}
