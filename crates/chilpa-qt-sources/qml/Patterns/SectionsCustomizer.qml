import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// Which shelves a page shows, in what order and how many items each: a
// toggle, up and down and a size per shelf, and the way back to how they
// came. A source's discovery page and the Home share it; each keeps its own
// choice and says here what changed.
Modal {
    id: customization
    closePolicy: Popup.CloseOnEscape
    onOpened: if (grouped) activeGroup = "chilpa"

    // [{id, title, enabled, size}], in the order shown.
    property var sections: []
    property var groupSources: []
    property var sizes: [{value: "6", text: "6"}, {value: "12", text: "12"}, {value: "18", text: "18"}, {value: "24", text: "24"}]
    property bool grouped: false
    property string activeGroup: "chilpa"
    readonly property var groups: {
        const rows = [{id: "chilpa", name: "Chilpa", icon: ""}]
        for (const source of groupSources) {
            if (source.id && !rows.some(row => row.id === source.id))
                rows.push({id: source.id, name: source.name || source.id, icon: source.icon || ""})
        }
        for (const section of sections) {
            if (section.sourceId && !rows.some(row => row.id === section.sourceId))
                rows.push({id: section.sourceId, name: section.sourceName || section.sourceId, icon: section.sourceIcon || ""})
        }
        return rows
    }
    readonly property var visibleRows: sections.map((section, sectionIndex) => Object.assign({}, section, {sectionIndex}))
        .filter(section => !grouped || (section.sourceId || "chilpa") === activeGroup)
    // The size a shelf has before one is chosen.
    property int defaultSize: Metrics.shelfItems

    signal toggled(int index, bool on)
    signal sized(int index, int size)
    signal moved(int index, int delta)
    signal restored()

    width: Math.min(Metrics.settingsMaxWidth, parent.width - Metrics.modalInsets)
    height: Math.min(parent.height - Metrics.modalInsets, Metrics.heroArt * 3) // derived: bounded dialog with a scrolling list
    Column {
        anchors.fill: parent; spacing: Metrics.gap16
        Text { id: title; text: I18n.tr("Customize", I18n.rev); font.pointSize: Type.titleSmall; color: Theme.color.textPrimary }
        Flickable {
            id: tabs
            visible: customization.grouped
            width: parent.width; height: visible ? Metrics.button32 : 0
            contentWidth: tabRow.width; contentHeight: height
            clip: true; interactive: contentWidth > width
            Row {
                id: tabRow; spacing: Metrics.gap6
                Repeater {
                    model: customization.groups
                    Rectangle {
                        required property var modelData
                        objectName: "customizeGroup_" + modelData.id
                        width: tabLabel.implicitWidth + Metrics.gap12 + Metrics.gap12
                        height: tabs.height; radius: Metrics.radiusControl
                        color: customization.activeGroup === modelData.id ? Theme.color.stateSelected : Theme.color.surfaceRaised
                        Row {
                            id: tabLabel; anchors.centerIn: parent; spacing: Metrics.gap6
                            SourceIcon { visible: !!modelData.icon; sourceInfo: ({icon: modelData.icon || ""}); width: visible ? Metrics.iconSm : 0; height: Metrics.iconSm }
                            Text { anchors.verticalCenter: parent.verticalCenter; text: modelData.name; font.pointSize: Type.body; color: Theme.color.textPrimary }
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: customization.activeGroup = modelData.id
                        }
                    }
                }
            }
        }
        ListView {
            id: choices
            objectName: "customizeChoices"
            width: parent.width; height: parent.height - title.height - tabs.height - actions.height - parent.spacing - parent.spacing - (tabs.visible ? parent.spacing : 0) // derived: fixed header and footer
            clip: true; spacing: Metrics.gap8; model: customization.visibleRows
            ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8 }
            Text {
                anchors.centerIn: parent
                visible: customization.grouped && customization.visibleRows.length === 0
                text: I18n.tr("No Home modules are available for this source.", I18n.rev)
                font.pointSize: Type.body; color: Theme.color.textSecondary
            }
            delegate: Row {
                required property var modelData
                required property int index
                readonly property int sectionIndex: modelData.sectionIndex
                width: choices.width - Metrics.gap16; spacing: Metrics.gap8
                Toggle { id: enabledToggle; checked: modelData.enabled !== false; onToggled: on => customization.toggled(sectionIndex, on) }
                SourceIcon { id: sourceLogo; anchors.verticalCenter: parent.verticalCenter; visible: !!modelData.sourceIcon; sourceInfo: ({icon: modelData.sourceIcon || ""}); width: visible ? Metrics.iconSm : 0; height: Metrics.iconSm }
                Text { width: parent.width - enabledToggle.width - sourceLogo.width - up.width - down.width - count.width - parent.spacing * 5; text: modelData.configTitle || modelData.title || ""; elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.body; height: count.height; verticalAlignment: Text.AlignVCenter } // derived: flexible label between fixed controls
                IconButton { id: up; glyph: Icons.chevronUp; tip: I18n.tr("Move up", I18n.rev); enabled: index > 0; onClicked: customization.moved(sectionIndex, customization.visibleRows[index - 1].sectionIndex - sectionIndex) }
                IconButton { id: down; glyph: Icons.chevronDown; tip: I18n.tr("Move down", I18n.rev); enabled: index < customization.visibleRows.length - 1; onClicked: customization.moved(sectionIndex, customization.visibleRows[index + 1].sectionIndex - sectionIndex) }
                SortButton { id: count; directed: false; value: String(modelData.size || customization.defaultSize); options: customization.sizes; onPicked: value => customization.sized(sectionIndex, Number(value)) }
            }
        }
        Row {
            id: actions; spacing: Metrics.gap8
            PillButton { text: I18n.tr("Restore defaults", I18n.rev); onClicked: customization.restored() }
            PillButton { text: I18n.tr("Close", I18n.rev); onClicked: customization.close() }
        }
    }
}
