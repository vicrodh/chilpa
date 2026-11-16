import QtQuick
import QtQuick.Controls.Basic as Controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import app.chilpa.dsp
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Modal {
    id: panel
    objectName: "dspPanel"
    preferredWidth: Metrics.dspPanelWidth
    preferredHeight: Metrics.dspPanelHeight
    heightRatio: Metrics.dspPanelHeightRatio
    readonly property var state: JSON.parse(Dsp.snapshot || "{}")
    readonly property var selection: state.selection || ({})
    readonly property string presetData: JSON.stringify(state.preset || {})
    property var draft: ({})
    property string group: "all"
    property string query: ""
    property string section: "graphic"
    property string importName: ""
    property string importText: ""
    property bool watching: false
    property bool closing: false
    readonly property bool wide: width >= Metrics.dspSidebarThreshold
    readonly property var sections: [
        {key: "graphic", label: I18n.tr("Graphic EQ", I18n.rev), glyph: Icons.adjustmentsHorizontal},
        {key: "tone", label: I18n.tr("Tone", I18n.rev), glyph: Icons.volume},
        {key: "parametric", label: I18n.tr("Parametric EQ", I18n.rev), glyph: Icons.waveSine},
        {key: "headroom", label: I18n.tr("Headroom", I18n.rev), glyph: Icons.volume2},
        {key: "stereo", label: I18n.tr("Stereo and crossfeed", I18n.rev), glyph: Icons.deviceSpeaker},
        {key: "dynamics", label: I18n.tr("Dynamics", I18n.rev), glyph: Icons.chartBar},
        {key: "presets", label: I18n.tr("Presets", I18n.rev), glyph: Icons.books},
        {key: "manage", label: I18n.tr("Personal presets", I18n.rev), glyph: Icons.user},
        {key: "assignments", label: I18n.tr("Output assignments", I18n.rev), glyph: Icons.deviceDesktop},
        {key: "import", label: I18n.tr("Import / export", I18n.rev), glyph: Icons.arrowsSort}
    ]
    readonly property string sectionTitle: sections.find(entry => entry.key === section)?.label || ""
    readonly property string presetsData: JSON.stringify(state.presets || [])
    readonly property var allPresets: JSON.parse(presetsData)
    readonly property var favorites: state.favorites || []
    readonly property string selectedKey: (selection.plugin || "") + "/" + (selection.preset || "")
    readonly property var browser: state.browser || ({})
    readonly property var shown: browser.presets || []
    DspEditQueue { id: edits; onCommand: (action, data) => panel.send(action, data) }
    Clipboard { id: clipboard }
    function requestSearch(offset) { send("search", {query: query, group: group, offset: offset}) }
    function copy(ids) { const text = Dsp.exportPresets(JSON.stringify(ids)); if (text) clipboard.write(text) }
    onGroupChanged: if (section === "presets" && visible) requestSearch(0)
    onSectionChanged: { edits?.flush(); if (section === "presets" && visible) requestSearch(0) }
    function finish() {
        if (closing || !watching) return
        closing = true
        edits.flush()
        send("close", {})
    }
    onAboutToHide: finish()
    function watch(wanted) {
        if (watching === wanted) return
        watching = wanted
        Dsp.watchView(wanted)
    }
    onAboutToShow: { closing = false; watch(true) }
    Connections { target: panel; function onClosed() { panel.watch(false) } }
    Component.onDestruction: { finish(); watch(false) }

    function send(action, data) { Dsp.command(action, JSON.stringify(data || {})) }
    function edit(value) { edits.commit("edit", value) }
    function choose(entry) { edits.cancel(); send("choose", {plugin: entry.plugin, preset: entry.preset.id}) }
    onPresetDataChanged: draft = JSON.parse(presetData)
    onOpened: {
        draft = JSON.parse(presetData)
        section = state.graphic ? "graphic" : "parametric"
    }

    Column {
        id: layout
        width: parent.width
        height: parent.height
        spacing: Metrics.gap16
        DspHeader {
            id: header
            width: parent.width
            processingEnabled: !!panel.selection.enabled
            available: Dsp.available
            onEnabledChangedByUser: on => { edits.cancel(); panel.send("enable", {enabled: on}) }
            onDismissed: panel.close()
        }
        Rectangle { width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
        Row {
            id: workspace
            width: parent.width
            height: Math.max(0, layout.height - header.height - footer.height - Metrics.hairline * 2 - layout.spacing * 4) // derived: remaining space between fixed header and footer
            spacing: panel.wide ? Metrics.gap24 : 0
            DspNavigation {
                visible: panel.wide
                width: visible ? Metrics.dspSidebarWidth : 0
                height: parent.height
                sections: panel.sections
                section: panel.section
                onSelected: section => panel.section = section
            }
            Column {
                id: page
                width: workspace.width - (panel.wide ? Metrics.dspSidebarWidth + workspace.spacing : 0)
                height: workspace.height
                spacing: Metrics.gap16
                SelectField {
                    id: compactNavigation
                    objectName: "dspCompactNavigation"
                    visible: !panel.wide
                    width: parent.width
                    options: panel.sections.map(entry => entry.label)
                    values: panel.sections.map(entry => entry.key)
                    value: panel.section
                    onPicked: value => panel.section = value
                }
                DspPresetBar {
                    id: presets
                    width: parent.width
                    presets: panel.allPresets
                    selectedName: panel.draft.name || ""
                    selectedCategory: panel.draft.category || ""
                    favorites: panel.favorites
                    selectedKey: panel.selectedKey
                    edited: !!panel.selection.edited
                    onChosen: entry => panel.choose(entry)
                    onFavoriteRequested: panel.send("favorite", {key: panel.selectedKey})
                }
                Text {
                    id: title
                    width: parent.width
                    text: panel.sectionTitle
                    elide: Text.ElideRight
                    font.pointSize: Type.section
                    font.weight: Type.semibold
                    color: Theme.color.textPrimary
                }
                Loader {
                    id: content
                    objectName: "dspPageLoader"
                    width: parent.width
                    height: Math.max(0, page.height - presets.height - title.height - (compactNavigation.visible ? compactNavigation.height + page.spacing : 0) - page.spacing * 2) // derived: only the active section scrolls
                    active: panel.visible
                    sourceComponent: panel.section === "presets" ? presetPage : editorPage
                }
            }
        }
        Rectangle { width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
        DspFooter {
            id: footer
            width: parent.width
            state: panel.state
            selection: panel.selection
            active: Dsp.active
            presetName: panel.draft.name || ""

        }
    }
    Component {
        id: presetPage
        DspPresets {
            catalog: panel.browser
            group: panel.group
            query: panel.query
            selectedKey: panel.selectedKey
            selectedDescription: panel.draft.description || ""
            onGroupPicked: value => panel.group = value
            onQueryEdited: value => panel.query = value
            onSearchRequested: offset => panel.requestSearch(offset)
            onChosen: entry => panel.choose(entry)
            onChecked: (id, selected) => panel.send("select", {id: id, selected: selected})
            onCopyRequested: ids => panel.copy(ids)
            onRemoveRequested: ids => panel.send("delete_many", {ids: ids})
        }
    }
    Component {
        id: editorPage
        DspEditor {
            state: panel.state
            draft: panel.draft
            section: panel.section
            importName: panel.importName
            importText: panel.importText
            maximumCurveHeight: Math.min(Metrics.dspCurveHeight, content.height * Metrics.dspCurveHeightShare)
            onImportNameEdited: value => panel.importName = value
            onImportTextEdited: value => panel.importText = value
            onCommand: (action, data) => edits.commit(action, data)
            onPreviewCommand: (action, data) => edits.preview(action, data)
            onPreviewed: value => edits.preview("preview", value)
            onEdited: value => panel.edit(value)
            onSectionRequested: section => panel.section = section
            onFindRequested: query => { panel.query = query; panel.group = "Headphones"; panel.section = "presets"; panel.requestSearch(0) }
        }
    }
}
