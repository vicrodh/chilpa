import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.dsp
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Modal {
    id: panel
    objectName: "dspPanel"
    width: Math.min(Metrics.dspPanelWidth, parent ? parent.width - Metrics.modalInsets : Metrics.dspPanelWidth)
    height: Math.min(Metrics.dspPanelHeight, parent ? parent.height - Metrics.modalInsets : Metrics.dspPanelHeight)
    readonly property var state: JSON.parse(Dsp.snapshot || "{}")
    readonly property var selection: state.selection || ({})
    readonly property string presetData: JSON.stringify(state.preset || {})
    property var draft: ({})
    property string group: "all"
    property int tab: 0
    readonly property var tabs: [I18n.tr("Graphic EQ", I18n.rev), I18n.tr("Advanced", I18n.rev), I18n.tr("Import / export", I18n.rev)]
    readonly property var allPresets: state.presets || []
    readonly property var favorites: state.favorites || []
    readonly property string selectedKey: (selection.plugin || "") + "/" + (selection.preset || "")
    readonly property var groups: ["all", "favorites"].concat([...new Set(allPresets.map(e => e.preset.category))])
    onGroupsChanged: if (!groups.includes(group)) group = "all"
    readonly property var shown: allPresets.filter(e => {
        const key = e.plugin + "/" + e.preset.id
        const matches = group === "all" || (group === "favorites" ? favorites.includes(key) : e.preset.category === group)
        const text = (e.preset.name + " " + e.preset.category + " " + e.preset.description).toLowerCase()
        return matches && text.includes(search.text.toLowerCase())
    }).sort((a, b) => Number(favorites.includes(b.plugin + "/" + b.preset.id)) - Number(favorites.includes(a.plugin + "/" + a.preset.id)))

    function send(action, data) { Dsp.command(action, JSON.stringify(data || {})) }
    function edit(value) { send("edit", value) }
    onPresetDataChanged: draft = JSON.parse(presetData)
    onOpened: { Dsp.refresh(); draft = JSON.parse(presetData) }
    onClosed: tab = 0

    Column {
        width: panel.availableWidth - Metrics.modalInsets
        height: panel.availableHeight - Metrics.modalInsets
        spacing: Metrics.gap10
        Row {
            width: parent.width
            spacing: Metrics.gap10
            Text {
                width: parent.width - enabledSwitch.width - closeButton.width - parent.spacing * 2 // derived: title beside two controls
                text: I18n.tr("Equalizer", I18n.rev)
                font.pointSize: Type.section
                font.weight: Type.semibold
                color: Theme.color.textPrimary
            }
            Toggle {
                id: enabledSwitch
                objectName: "dspEnabled"
                checked: !!panel.selection.enabled
                enabled: Dsp.available
                Accessible.name: I18n.tr("Enable audio processing; turn off for bypass", I18n.rev)
                onToggled: on => panel.send("enable", { enabled: on })
            }
            IconButton { id: closeButton; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: panel.close(); activeFocusOnTab: true; Accessible.name: tip; Accessible.role: Accessible.Button; Keys.onReturnPressed: clicked(); Keys.onSpacePressed: clicked() }
        }
        Text {
            objectName: "dspDisclaimer"
            width: parent.width
            text: I18n.tr("DSP changes the signal and breaks strict bit-perfect playback.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Text {
            width: parent.width
            text: panel.state.blocked || ((Dsp.active ? I18n.tr("DSP active", I18n.rev) : panel.selection.enabled ? I18n.tr("Ready for local PCM playback", I18n.rev) : I18n.tr("Bypass · original samples", I18n.rev)) + (panel.draft.name ? " · " + panel.draft.name : ""))
            wrapMode: Text.WordWrap
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
        Row {
            width: parent.width
            spacing: Metrics.gap8
            SelectField {
                width: parent.width / 2 - parent.spacing // derived: two search controls
                options: panel.groups.map(g => g === "all" ? I18n.tr("All groups", I18n.rev) : g === "favorites" ? I18n.tr("Favorites", I18n.rev) : g)
                values: panel.groups
                value: panel.group
                onPicked: value => panel.group = value
            }
            SearchField {
                id: search
                objectName: "dspSearch"
                width: parent.width / 2 // derived: two search controls
                placeholder: I18n.tr("Find a preset or device", I18n.rev)
            }
        }
        Row {
            width: parent.width
            spacing: Metrics.gap8
            SelectField {
                objectName: "dspPresetPicker"
                width: parent.width - favoriteButton.width - parent.spacing
                menuWidthLimit: Metrics.dspPanelWidth - Metrics.modalInsets
                options: panel.shown.map(e => e.preset.name)
                details: panel.shown.map(e => e.preset.category)
                values: panel.shown.map(e => e.plugin + "/" + e.preset.id)
                value: panel.selectedKey
                onPicked: value => {
                    const entry = panel.shown.find(e => e.plugin + "/" + e.preset.id === value)
                    if (entry) panel.send("choose", { plugin: entry.plugin, preset: entry.preset.id })
                }
            }
            IconButton {
                id: favoriteButton
                objectName: "dspFavorite"
                glyph: Icons.star
                tip: I18n.tr("Favorite preset", I18n.rev)
                checked: panel.favorites.includes(panel.selectedKey)
                enabled: !!panel.selection.preset && !panel.selection.edited
                activeFocusOnTab: enabled
                Accessible.name: tip
                Accessible.role: Accessible.Button
                Keys.onReturnPressed: clicked()
                Keys.onSpacePressed: clicked()
                onClicked: panel.send("favorite", { key: panel.selectedKey })
            }
        }
        Text {
            width: parent.width
            text: panel.shown.length === 0 ? I18n.tr("No presets match this search.", I18n.rev) : panel.draft.description || ""
            wrapMode: Text.WordWrap
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
        Text {
            width: parent.width
            visible: !!panel.draft.source
            text: panel.draft.source || ""
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
        SegmentedControl {
            objectName: "dspTabs"
            visible: panel.width >= Metrics.dspCompactWidth
            current: panel.tab
            options: panel.tabs.map(text => ({text: text}))
            onActivated: index => panel.tab = index
        }
        SelectField {
            visible: panel.width < Metrics.dspCompactWidth
            width: parent.width
            options: panel.tabs
            values: ["0", "1", "2"]
            value: String(panel.tab)
            onPicked: value => panel.tab = Number(value)
        }
        Controls.ScrollView {
            width: parent.width
            height: Math.max(Metrics.fieldHeight, parent.height - y - footer.height - Metrics.gap10)
            contentWidth: availableWidth
            clip: true
            DspEditor {
                width: parent.width
                state: panel.state
                draft: panel.draft
                tab: panel.tab
                onCommand: (action, data) => panel.send(action, data)
                onEdited: value => panel.edit(value)
                onTabRequested: tab => panel.tab = tab
            }
        }
        Column {
            id: footer
            width: parent.width
            spacing: Metrics.gap6
            Text {
                width: parent.width
                visible: !!panel.state.error
                text: panel.state.error || ""
                wrapMode: Text.WordWrap
                color: Theme.color.textSecondary
                font.pointSize: Type.small
            }
            Row {
                width: parent.width
                spacing: Metrics.gap8
                TextField { id: saveName; width: parent.width - saveButton.width - deleteButton.width - parent.spacing * 2; placeholder: I18n.tr("Name a personal preset", I18n.rev); Accessible.name: placeholder } // derived: room for two buttons
                PillButton { id: saveButton; text: I18n.tr("Save preset", I18n.rev); enabled: saveName.text.trim() !== ""; onClicked: panel.send("save", { name: saveName.text }) }
                IconButton { id: deleteButton; glyph: Icons.trash; tip: I18n.tr("Delete preset", I18n.rev); enabled: (panel.selection.preset || "").startsWith("user-"); onClicked: panel.send("delete", {}); activeFocusOnTab: enabled; Accessible.name: tip; Accessible.role: Accessible.Button; Keys.onReturnPressed: clicked(); Keys.onSpacePressed: clicked() }
            }
        }
    }
}
