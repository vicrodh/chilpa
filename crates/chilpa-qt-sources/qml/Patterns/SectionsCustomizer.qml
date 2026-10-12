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

    // [{id, title, enabled, size}], in the order shown.
    property var sections: []
    property var sizes: [{value: "6", text: "6"}, {value: "12", text: "12"}, {value: "18", text: "18"}, {value: "24", text: "24"}]
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
        ListView {
            id: choices
            objectName: "customizeChoices"
            width: parent.width; height: parent.height - title.height - actions.height - parent.spacing * 2 // derived: fixed header and footer
            clip: true; spacing: Metrics.gap8; model: customization.sections
            ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8 }
            delegate: Row {
                required property var modelData
                required property int index
                width: choices.width - Metrics.gap16; spacing: Metrics.gap8
                Toggle { id: enabledToggle; checked: modelData.enabled !== false; onToggled: on => customization.toggled(index, on) }
                Text { width: parent.width - enabledToggle.width - up.width - down.width - count.width - parent.spacing * 4; text: modelData.title || ""; elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.body; height: count.height; verticalAlignment: Text.AlignVCenter } // derived: flexible label between fixed controls
                IconButton { id: up; glyph: Icons.chevronUp; tip: I18n.tr("Move up", I18n.rev); enabled: index > 0; onClicked: customization.moved(index, -1) }
                IconButton { id: down; glyph: Icons.chevronDown; tip: I18n.tr("Move down", I18n.rev); enabled: index < customization.sections.length - 1; onClicked: customization.moved(index, 1) }
                SortButton { id: count; directed: false; value: String(modelData.size || customization.defaultSize); options: customization.sizes; onPicked: value => customization.sized(index, Number(value)) }
            }
        }
        Row {
            id: actions; spacing: Metrics.gap8
            PillButton { text: I18n.tr("Restore defaults", I18n.rev); onClicked: customization.restored() }
            PillButton { text: I18n.tr("Close", I18n.rev); onClicked: customization.close() }
        }
    }
}
