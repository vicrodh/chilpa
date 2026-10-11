import QtQuick
import QtQuick.Controls.Basic as Controls
import QtQuick.Effects
import "../Theme"

// I keep original entry indices when filtering so labels never change identity.
Controls.Popup {
    id: menu
    modal: true
    dim: false
    property var entries: []
    // A binding and not an alias: Qt 6.11.1 refuses an alias into another
    // object's list, and the whole window failed to load on a Mac.
    readonly property var entryItems: list.contentItem ? list.contentItem.children : []
    property string title
    property bool searchable: false
    property real heightLimit: Infinity
    property real widthLimit: Metrics.menuMaxWidth
    property Item hotRow: null
    readonly property bool hovering: panelHover.hovered || !!(hotRow && hotRow.hovered)
    readonly property var shown: entries.map((entry, index) => ({entry: entry, original: index})).filter(row => !searchable || !search.text || [row.entry.text, row.entry.detail].join(" ").toLocaleLowerCase().indexOf(search.text.toLocaleLowerCase()) >= 0)
    signal entryHovered(int index, real top)
    signal chosen(int index)
    signal submenu(int index, real top)
    property real widest: Metrics.menuWidth
    TextMetrics { id: textMeasure; font.pointSize: Type.body }
    onEntriesChanged: measure()
    function measure() {
        let width = Metrics.menuWidth
        for (const entry of entries) {
            textMeasure.text = [entry.text, entry.detail, entry.mark, entry.hint].filter(Boolean).join(" ")
            width = Math.max(width, textMeasure.width + Metrics.menuIconColumn + Metrics.gap16 * 4) // derived: icons and row padding
        }
        widest = width
    }
    function choose(index, top) {
        const row = shown[index]
        if (!row || row.entry.separator || row.entry.note || row.entry.enabled === false) return
        if (row.entry.submenu) { submenu(row.original, top); return }
        close()
        chosen(row.original)
    }
    onAboutToShow: {
        TipState.hide(TipState.target)
        search.text = ""
        list.forceLayout()
        list.currentIndex = Math.max(0, shown.findIndex(row => row.entry.checked))
        if (searchable) Qt.callLater(() => search.focusInput())
    }
    padding: Metrics.gap4
    margins: Metrics.gap10
    implicitWidth: Math.min(widthLimit, widest + padding * 2) // derived: panel padding
    background: Item {
        Hold {}
        HoverHandler { id: panelHover }
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowElevatedY
            blur: Metrics.shadowElevatedBlur
            color: Theme.color.shadowElevated
        }
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowKeyY
            blur: Metrics.shadowKeyBlur
            color: Theme.color.shadowElevatedKey
        }
        Rectangle {
            id: panel
            anchors.fill: parent
            radius: Metrics.radiusMenu
            color: Theme.color.surfaceRaised

            Edge {}
        }
    }

    contentItem: Column {
        readonly property real contentHeight: list.contentHeight
        property alias contentY: list.contentY
        spacing: Metrics.gap4
        Text {
            visible: menu.title !== ""
            width: parent.width
            height: Metrics.menuItem
            leftPadding: Metrics.gap10
            verticalAlignment: Text.AlignVCenter
            text: menu.title
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
        SearchField {
            id: search
            objectName: "optionSearch"
            visible: menu.searchable
            width: parent.width
            placeholder: Theme.searchLabel
            onTextChanged: list.currentIndex = 0
            onStepped: by => { list.currentIndex = Math.max(0, Math.min(list.count - 1, list.currentIndex + by)); list.positionViewAtIndex(list.currentIndex, ListView.Contain) }
            onAccepted: menu.choose(list.currentIndex, 0)
            onEscaped: menu.close()
        }
        ListView {
            id: list
            width: parent.width
            implicitHeight: Math.min(contentHeight, menu.heightLimit, (menu.Controls.Overlay.overlay ? menu.Controls.Overlay.overlay.height : Metrics.menuMaxWidth) - Metrics.fieldHeightLarge * 3) // derived: space for search and popup margins
            model: menu.shown
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            Controls.ScrollBar.vertical: Controls.ScrollBar {}
            delegate: MenuEntry {
                required property int index
                required property var modelData
                width: list.width
                entry: Object.assign({}, modelData.entry, {focused: menu.searchable && index === list.currentIndex})
                onHoveredChanged: if (hovered) { menu.hotRow = this; list.currentIndex = index; menu.entryHovered(modelData.original, mapToItem(menu.contentItem, 0, 0).y) }
                onClicked: menu.choose(index, menu.y + mapToItem(menu.contentItem, 0, 0).y)
            }
        }
        Text {
            visible: menu.searchable && list.count === 0
            text: Theme.emptySearchLabel
            color: Theme.color.textSecondary
            font.pointSize: Type.body
            padding: Metrics.gap10
        }
    }
}
