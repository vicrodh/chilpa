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
    property var sizes: [{value: "0", text: I18n.tr("One row", I18n.rev)}, {value: "6", text: "6"}, {value: "12", text: "12"}, {value: "18", text: "18"}, {value: "24", text: "24"}]
    // How a shelf that can be a compact grid is drawn (the Home's).
    readonly property var modes: [{value: "compact", text: I18n.tr("Compact", I18n.rev)}, {value: "cards", text: I18n.tr("Cards", I18n.rev)}]
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
    readonly property var activeRows: sections.map((section, sectionIndex) => Object.assign({}, section, {sectionIndex}))
        .filter(section => section.enabled !== false)
    // The size a shelf has before one is chosen.
    property int defaultSize: 0
    // The widest of each choice, so every row keeps its buttons in the same
    // columns, as QBZ's rows do with a select of one width.
    readonly property bool anyCompact: sections.some(section => section.compact === true)
    readonly property real modeSlot: widest(modeProbes)
    readonly property real sizeSlot: widest(sizeProbes)
    function widest(probes) {
        let most = 0
        for (let i = 0; i < probes.count; ++i) most = Math.max(most, probes.itemAt(i) ? probes.itemAt(i).implicitWidth : 0)
        return most
    }
    Repeater { id: modeProbes; model: customization.modes; PillButton { visible: false; glyph: Icons.layoutGrid; text: modelData.text } }
    Repeater { id: sizeProbes; model: customization.sizes; PillButton { visible: false; glyph: Icons.arrowsSort; text: modelData.text } }

    signal toggled(int index, bool on)
    signal sized(int index, int size)
    signal laidOut(int index, string mode)
    signal moved(int index, int delta)
    signal restored()

    width: Math.min(grouped ? Metrics.settingsMaxWidth + Metrics.heroArt : Metrics.settingsMaxWidth,
                    parent.width - Metrics.modalInsets)
    height: Math.min(parent.height - Metrics.modalInsets, Metrics.heroArt * 3) // derived: bounded dialog with a scrolling list
    Column {
        anchors.fill: parent; spacing: Metrics.gap16
        Text { id: title; text: I18n.tr("Customize", I18n.rev); font.pointSize: Type.titleSmall; color: Theme.color.textPrimary }
        Row {
            id: columns
            visible: customization.grouped
            width: parent.width
            height: parent.height - title.height - actions.height - parent.spacing * 2 // derived: two gaps between title, columns and actions
            spacing: Metrics.gap16
            Column {
                width: (columns.width - columns.spacing) / 2 // derived: two equally wide columns
                height: parent.height
                spacing: Metrics.gap8
                Flickable {
                    id: tabs
                    width: parent.width; height: Metrics.button32
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
                                HoverHandler { cursorShape: Qt.PointingHandCursor }
                                TapHandler { onTapped: customization.activeGroup = modelData.id }
                            }
                        }
                    }
                }
                ListView {
                    id: sourceChoices
                    objectName: "customizeSourceChoices"
                    width: parent.width; height: parent.height - tabs.height - parent.spacing
                    clip: true; spacing: Metrics.gap8; model: customization.visibleRows
                    ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8 }
                    Text {
                        anchors.centerIn: parent
                        visible: customization.visibleRows.length === 0
                        text: I18n.tr("No Home modules are available for this source.", I18n.rev)
                        font.pointSize: Type.body; color: Theme.color.textSecondary
                    }
                    delegate: Row {
                        required property var modelData
                        objectName: "sourceModule_" + modelData.id
                        width: sourceChoices.width - Metrics.gap16; spacing: Metrics.gap8
                        Toggle { id: sourceToggle; checked: modelData.enabled !== false; onToggled: on => customization.toggled(modelData.sectionIndex, on) }
                        Text {
                            width: parent.width - sourceToggle.width - parent.spacing
                            height: sourceToggle.height; verticalAlignment: Text.AlignVCenter
                            text: modelData.configTitle || modelData.title || ""
                            elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.body
                        }
                    }
                }
            }
            Column {
                width: (columns.width - columns.spacing) / 2 // derived: two equally wide columns
                height: parent.height
                spacing: Metrics.gap8
                Text {
                    height: Metrics.button32
                    verticalAlignment: Text.AlignVCenter
                    text: I18n.tr("Active modules", I18n.rev)
                    font.pointSize: Type.body; font.weight: Type.bold; color: Theme.color.textPrimary
                }
                ListView {
                    id: activeChoices
                    objectName: "customizeActiveModules"
                    width: parent.width; height: parent.height - Metrics.button32 - parent.spacing
                    clip: true; spacing: Metrics.gap8; model: customization.activeRows
                    ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8 }
                    delegate: Item {
                        required property var modelData
                        required property int index
                        readonly property int sectionIndex: modelData.sectionIndex
                        objectName: "activeModule_" + modelData.id
                        width: activeChoices.width - Metrics.gap16; height: activeControls.height
                        SourceIcon {
                            id: activeLogo; anchors.verticalCenter: parent.verticalCenter
                            visible: !!modelData.sourceIcon
                            sourceInfo: ({icon: modelData.sourceIcon || ""})
                            width: visible ? Metrics.iconSm : 0; height: Metrics.iconSm
                        }
                        Text {
                            x: activeLogo.visible ? activeLogo.width + Metrics.gap8 : 0
                            width: activeControls.x - x - Metrics.gap8
                            height: parent.height; verticalAlignment: Text.AlignVCenter
                            text: modelData.configTitle || modelData.title || ""
                            elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.body
                        }
                        Row {
                            id: activeControls
                            anchors.right: parent.right; spacing: Metrics.gap8
                            IconButton {
                                id: activeUp; objectName: "activeMoveUp_" + modelData.id
                                anchors.verticalCenter: parent.verticalCenter
                                glyph: Icons.chevronUp; tip: I18n.tr("Move up", I18n.rev)
                                enabled: index > 0
                                onClicked: customization.moved(sectionIndex, customization.activeRows[index - 1].sectionIndex - sectionIndex)
                            }
                            IconButton {
                                id: activeDown; objectName: "activeMoveDown_" + modelData.id
                                anchors.verticalCenter: parent.verticalCenter
                                glyph: Icons.chevronDown; tip: I18n.tr("Move down", I18n.rev)
                                enabled: index < customization.activeRows.length - 1
                                onClicked: customization.moved(sectionIndex, customization.activeRows[index + 1].sectionIndex - sectionIndex)
                            }
                            Item {
                                visible: customization.anyCompact
                                width: customization.modeSlot; height: activeCount.height
                                SortButton {
                                    id: activeMode; objectName: "activeMode_" + modelData.id
                                    visible: modelData.compact === true
                                    directed: false; glyph: Icons.layoutGrid; value: modelData.mode || "compact"
                                    options: customization.modes
                                    onPicked: value => customization.laidOut(sectionIndex, value)
                                }
                            }
                            Item {
                                width: customization.sizeSlot; height: activeCount.height
                                SortButton {
                                    id: activeCount; objectName: "activeCount_" + modelData.id
                                    directed: false; value: String(modelData.size || customization.defaultSize)
                                    options: customization.sizes
                                    onPicked: value => customization.sized(sectionIndex, Number(value))
                                }
                            }
                        }
                    }
                }
            }
        }
        ListView {
            id: choices
            objectName: "customizeChoices"
            visible: !customization.grouped
            width: parent.width; height: visible ? parent.height - title.height - actions.height - parent.spacing * 2 : 0 // derived: two gaps between title, list and actions
            clip: true; spacing: Metrics.gap8; model: customization.visibleRows
            ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8 }
            delegate: Row {
                required property var modelData
                required property int index
                readonly property int sectionIndex: modelData.sectionIndex
                width: choices.width - Metrics.gap16; spacing: Metrics.gap8
                Toggle { id: enabledToggle; checked: modelData.enabled !== false; onToggled: on => customization.toggled(sectionIndex, on) }
                SourceIcon { id: sourceLogo; anchors.verticalCenter: parent.verticalCenter; visible: !!modelData.sourceIcon; sourceInfo: ({icon: modelData.sourceIcon || ""}); width: visible ? Metrics.iconSm : 0; height: Metrics.iconSm }
                Text { width: parent.width - enabledToggle.width - sourceLogo.width - up.width - down.width - countCell.width - parent.spacing * 5; text: modelData.configTitle || modelData.title || ""; elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.body; height: count.height; verticalAlignment: Text.AlignVCenter } // derived: flexible label between fixed controls
                IconButton { id: up; glyph: Icons.chevronUp; tip: I18n.tr("Move up", I18n.rev); enabled: index > 0; onClicked: customization.moved(sectionIndex, customization.visibleRows[index - 1].sectionIndex - sectionIndex) }
                IconButton { id: down; glyph: Icons.chevronDown; tip: I18n.tr("Move down", I18n.rev); enabled: index < customization.visibleRows.length - 1; onClicked: customization.moved(sectionIndex, customization.visibleRows[index + 1].sectionIndex - sectionIndex) }
                Item {
                    id: countCell; width: customization.sizeSlot; height: count.height
                    SortButton { id: count; directed: false; value: String(modelData.size || customization.defaultSize); options: customization.sizes; onPicked: value => customization.sized(sectionIndex, Number(value)) }
                }
            }
        }
        Row {
            id: actions; spacing: Metrics.gap8
            PillButton { text: I18n.tr("Restore defaults", I18n.rev); onClicked: customization.restored() }
            PillButton { text: I18n.tr("Close", I18n.rev); onClicked: customization.close() }
        }
    }
}
