import QtQuick
import QtQuick.Controls.Basic as Controls
import QtQuick.Dialogs
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"
import "../Components"

Item {
    id: page
    readonly property var document: JSON.parse(PlaylistImport.snapshot || "{}")
    readonly property var state: document.state || ({})
    readonly property var inputs: (document.options || {}).inputs || []
    readonly property var input: inputs.find(v => v.id === inputType.value) || ({})
    readonly property var destinations: [{id: "local", name: I18n.tr("Local only", I18n.rev)}].concat((document.sources || []).filter(v => v.writable), [{id: "mixed", name: I18n.tr("Any source — best quality (local mixed playlist)", I18n.rev)}])
    property var selected: ["mixed"]
    property string file: ""
    property string filter: "all"
    property int offset: 0
    property int step: 0
    readonly property bool busy: !!state.busy || !!document.pending
    property bool restored: false
    property string previousPhase: ""
    onStateChanged: {
        if (!restored && (state.targets || []).length > 0) { selected = state.targets.slice(); restored = true }
        if (state.phase === "done" && previousPhase !== "done") { filter = "unmatched"; offset = 0; send("status", {filter: filter, offset: 0}) }
        if (state.phase !== previousPhase) {
            if (state.phase === "preview") step = 1
            if (["matching", "ready", "creating", "adding"].indexOf(state.phase) >= 0) step = 2
            if (state.phase === "done") step = 3
            if (state.phase === "input") step = 0
            if (state.phase === "cancelled") step = state.preview ? 1 : 0
        }
        previousPhase = state.phase || ""
    }
    function destinationName(id) { return id === "mixed" ? I18n.tr("Mixed", I18n.rev) : id === "local" ? I18n.tr("Local only", I18n.rev) : ((document.sources || []).find(v => v.id === id) || {}).name || id }
    function quality(value) {
        const q = value || {}, fields = []
        if (q.codec) fields.push(q.codec.toUpperCase())
        if (q.tier === "hi_res") fields.push("Hi-Res")
        if (q.bits) fields.push(q.bits + " bit")
        if (q.rate) fields.push((q.rate / 1000) + " kHz")
        return fields.join(" · ") || I18n.tr("Quality unknown", I18n.rev)
    }
    function phaseName(phase) {
        const labels = {input: I18n.tr("Playlist origin", I18n.rev), fetching: I18n.tr("Reading playlist…", I18n.rev), preview: I18n.tr("Playlist preview", I18n.rev), matching: I18n.tr("Finding matches…", I18n.rev), ready: I18n.tr("Ready to review", I18n.rev), creating: I18n.tr("Creating playlists…", I18n.rev), adding: I18n.tr("Adding tracks…", I18n.rev), done: I18n.tr("Import complete", I18n.rev), cancelled: I18n.tr("Import cancelled", I18n.rev)}
        return labels[phase] || ""
    }
    function statusName(status) { return status === "unmatched" ? I18n.tr("Unmatched", I18n.rev) : status === "matched" ? I18n.tr("Matched", I18n.rev) : status === "error" ? I18n.tr("Catalog error", I18n.rev) : I18n.tr("Pending", I18n.rev) }
    function send(action, payload) { PlaylistImport.command(action, JSON.stringify(payload || {})) }
    function toggle(id) { selected = selected.indexOf(id) < 0 ? selected.concat([id]) : selected.filter(v => v !== id) }
    Component.onCompleted: PlaylistImport.refresh()
    Timer { interval: 500; running: page.visible; repeat: true; onTriggered: PlaylistImport.refresh() }
    FileDialog { id: picker; title: I18n.tr("Choose playlist file", I18n.rev); nameFilters: ["Playlist files (*.m3u *.m3u8 *.pls *.xspf *.json)", "All files (*)"]; onAccepted: page.file = selectedFile.toString() }
    Column {
        id: heading
        x: Metrics.contentX; y: Metrics.contentTop
        width: parent.width - Metrics.contentX * 2 // derived: padding on both sides
        spacing: Metrics.gap8
        Text { text: I18n.tr("Import playlist", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.title; font.weight: Type.bold }
        Text { width: parent.width; wrapMode: Text.Wrap; color: Theme.color.textSecondary; text: page.document.installed ? I18n.tr("Read a playlist, choose destinations, review matches, then create your playlists.", I18n.rev) : I18n.tr("Install and enable a playlist importer in Settings → Plugins.", I18n.rev) }
        Text { width: parent.width; visible: text !== ""; wrapMode: Text.Wrap; color: Theme.color.textPrimary; text: page.document.error || page.state.error || "" }
    }
    Flow {
        id: steps; x: Metrics.contentX; anchors.top: heading.bottom; anchors.topMargin: Metrics.gap16
        width: parent.width - Metrics.contentX * 2; spacing: Metrics.gap8 // derived: proportional space within the parent
        Repeater {
            model: [I18n.tr("1. Origin", I18n.rev), I18n.tr("2. Destinations", I18n.rev), I18n.tr("3. Review", I18n.rev), I18n.tr("4. Summary", I18n.rev)]
            PillButton { required property int index; required property string modelData; text: modelData; kind: page.step === index ? "primary" : "normal"; enabled: index === page.step || !page.busy && index < page.step && page.step < 3; onClicked: if (!page.busy && index < page.step && page.step < 3) page.step = index }
        }
    }
    Flickable {
        id: setup; visible: page.step < 2
        x: Metrics.contentX; anchors.top: steps.bottom; anchors.topMargin: Metrics.gap24; anchors.bottom: footer.top; anchors.bottomMargin: Metrics.gap16
        width: Math.min(Metrics.selectWidth * 2, page.width - Metrics.contentX * 2) // derived: bounded form with room for translated labels
        clip: true; contentHeight: form.height; contentWidth: width
        Controls.ScrollBar.vertical: ViewportScrollBar {}
        Column {
            id: form; width: setup.width - Metrics.gap8; spacing: Metrics.gap10; enabled: !!page.document.installed
            Column { width: parent.width; spacing: Metrics.gap12; visible: page.step === 0
            Text { text: I18n.tr("Playlist origin", I18n.rev); color: Theme.color.textPrimary; font.weight: Type.semibold }
            SelectField { id: inputType; width: parent.width; options: page.inputs.map(v => v.name); values: page.inputs.map(v => v.id); value: "url"; enabled: !page.state.busy; onPicked: selectedValue => value = selectedValue }
            Text { width: parent.width; wrapMode: Text.Wrap; text: page.input.hint || ""; color: Theme.color.textSecondary; font.pointSize: Type.small }
            TextField { id: origin; width: parent.width; visible: page.input.kind !== "file"; enabled: !page.state.busy; placeholder: page.input.name || "" }
            PillButton { visible: page.input.kind === "file"; text: I18n.tr("Choose file…", I18n.rev); enabled: !page.state.busy; onClicked: picker.open() }
            Text { width: parent.width; visible: page.input.kind === "file"; elide: Text.ElideMiddle; text: page.file; color: Theme.color.textSecondary }
            SelectField { id: station; visible: page.input.kind === "station"; width: parent.width; options: (page.input.choices || []).map(v => v.name); values: (page.input.choices || []).map(v => String(v.id)); value: "0"; onPicked: selectedValue => value = selectedValue }
            TextField { id: token; width: parent.width; visible: !!page.input.private_token; secret: true; placeholder: I18n.tr("Optional access token for private playlists", I18n.rev) }
            PillButton { visible: !!page.input.private_token; text: token.secret ? I18n.tr("Show token", I18n.rev) : I18n.tr("Hide token", I18n.rev); onClicked: token.secret = !token.secret }
            PillButton { visible: page.input.kind === "choices"; text: I18n.tr("Find playlists", I18n.rev); enabled: !page.state.busy; onClicked: page.send("choices", {text: origin.text, token: token.text}) }
            SelectField { id: choice; visible: page.input.kind === "choices" && ((page.document.choices || {}).choices || []).length > 0; width: parent.width; options: ((page.document.choices || {}).choices || []).map(v => v.name); values: ((page.document.choices || {}).choices || []).map(v => v.id); onPicked: selectedValue => value = selectedValue }
            }
            Column { width: parent.width; spacing: Metrics.gap12; visible: page.step === 1
            Text { text: I18n.tr("Destinations", I18n.rev); color: Theme.color.textPrimary; font.weight: Type.semibold }
            Repeater {
                model: page.destinations
                ImportCheck {
                    required property var modelData
                    width: parent.width; text: modelData.name; checked: page.selected.indexOf(modelData.id) >= 0; enabled: !page.state.busy
                    onToggled: page.toggle(modelData.id)
                }
            }
            Text { width: parent.width; wrapMode: Text.Wrap; color: Theme.color.textSecondary; font.pointSize: Type.small; text: I18n.tr("Each selected destination creates its own playlist. Mixed playlists stay in Chilpa and can contain tracks from every connected source.", I18n.rev) }
            TextField { id: title; width: parent.width; placeholder: I18n.tr("Playlist name", I18n.rev); text: (page.state.settings || {}).title || (page.state.preview || {}).name || ""; enabled: !page.state.busy }
            TextField { id: description; width: parent.width; placeholder: I18n.tr("Description", I18n.rev); text: (page.state.settings || {}).description || (page.state.preview || {}).description || ""; enabled: !page.state.busy }
            ImportCheck { id: publicPlaylist; text: I18n.tr("Public remote playlists", I18n.rev); enabled: !page.state.busy }
            SelectField { id: folder; width: parent.width; options: [I18n.tr("No folder", I18n.rev)].concat((page.document.folders || []).map(v => v.name)); values: ["0"].concat((page.document.folders || []).map(v => String(v.id))); value: "0"; onPicked: selectedValue => value = selectedValue }
            }
        }
    }
    ListView {
        id: review; visible: page.step >= 2
        x: Metrics.contentX; width: parent.width - Metrics.contentX * 2 // derived: page inset on both sides
        anchors.top: steps.bottom; anchors.topMargin: Metrics.gap24; anchors.bottom: footer.top; anchors.bottomMargin: Metrics.gap16
        header: Column {
        id: summary; width: review.width; spacing: Metrics.gap8; bottomPadding: Metrics.gap16
        Text { color: Theme.color.textPrimary; text: (page.state.preview || {}).name || I18n.tr("Track review", I18n.rev); font.pointSize: Type.section; elide: Text.ElideRight; width: parent.width }
        Text { width: parent.width; wrapMode: Text.Wrap; color: Theme.color.textSecondary; text: page.phaseName(page.state.phase) + " · " + (page.state.completed || 0) + " / " + (page.state.total_work || (page.state.preview || {}).total || 0) + (page.state.current ? " · " + page.state.current : "") }
        Repeater { model: page.state.warnings || []; Text { required property string modelData; width: summary.width; wrapMode: Text.Wrap; color: Theme.color.textSecondary; text: modelData } }
        Repeater {
            model: page.state.outputs || []
            Column { required property var modelData; width: summary.width; spacing: Metrics.gap4
            Text { width: parent.width; wrapMode: Text.Wrap; color: Theme.color.textPrimary; text: page.destinationName(modelData.target) + ": " + I18n.trArgs("{} written · {} unmatched · {} duplicates", [String(modelData.written), String(modelData.unmatched), String(modelData.duplicates)], I18n.rev) + (modelData.error ? "\n" + modelData.error : "") }
            Flow { width: parent.width; spacing: Metrics.gap8; property var output: parent.modelData
                Repeater { model: parent.output.parts || []; PillButton { required property var modelData; property var output: parent.output; text: modelData.title; onClicked: { if (output.target === "local" || output.target === "mixed") Playlists.open(Number(modelData.ref)); else Navigation.goKey("source", output.target + " native:" + modelData.ref) } } }
            }
            }
        }
        Flow {
            width: parent.width; spacing: Metrics.gap8
            SelectField { options: [I18n.tr("All", I18n.rev), I18n.tr("Unmatched", I18n.rev), I18n.tr("Errors", I18n.rev)]; values: ["all", "unmatched", "error"]; value: page.filter; onPicked: value => { page.filter = value; page.offset = 0; page.send("status", {filter: value, offset: 0}) } }
            PillButton { text: I18n.tr("Previous", I18n.rev); enabled: page.offset > 0; onClicked: { page.offset = Math.max(0, page.offset - 200); page.send("status", {filter: page.filter, offset: page.offset}) } }
            PillButton { text: I18n.tr("Next", I18n.rev); enabled: page.offset + 200 < (page.state.row_count || 0); onClicked: { page.offset += 200; page.send("status", {filter: page.filter, offset: page.offset}) } }
        }
    }
        clip: true; spacing: Metrics.gap8; model: page.state.rows || []
        Controls.ScrollBar.vertical: ViewportScrollBar {}
        delegate: Rectangle {
            required property var modelData
            width: ListView.view.width; height: row.height + Metrics.gap16; radius: Metrics.radiusControl; color: Theme.color.fillField
            Column {
                id: row; x: Metrics.gap8; y: Metrics.gap8; width: parent.width - Metrics.gap16; spacing: Metrics.gap4
                Text { width: parent.width; elide: Text.ElideRight; text: modelData.index + ". " + modelData.original.title + " — " + modelData.original.artist; color: Theme.color.textPrimary; font.weight: Type.semibold }
                Repeater {
                    model: Object.keys(modelData.matches || {})
                    ImportMatch {
                        required property string modelData
                        width: row.width; match: row.parent.modelData.matches[modelData]
                        destination: page.destinationName(modelData)
                        sourceName: page.destinationName(match.source)
                        qualityLabel: page.quality(match.quality)
                    }
                }
                Text { width: parent.width; wrapMode: Text.Wrap; color: Theme.color.textSecondary; text: page.statusName(modelData.status) + (Object.keys(modelData.errors || {}).length ? " · " + Object.values(modelData.errors).join(" · ") : "") }
            }
        }
    }
    Flow {
        width: parent.width - Metrics.contentX * 2 // derived: actions wrap within the page at minimum width
        id: footer; x: Metrics.contentX; anchors.bottom: parent.bottom; anchors.bottomMargin: Metrics.contentBottom
        spacing: Metrics.gap12
        PillButton { text: I18n.tr("Back", I18n.rev); visible: page.step > 0 && page.step < 3; enabled: !page.busy; onClicked: page.step-- }
        PillButton {
            objectName: "importPrimary"
            text: page.step === 0 ? I18n.tr("Read playlist", I18n.rev) : page.step === 1 ? I18n.tr("Find matches", I18n.rev) : page.step === 2 ? I18n.tr("Create playlists", I18n.rev) : I18n.tr("New import", I18n.rev)
            kind: "primary"
            enabled: !!page.document.installed && !page.busy && (page.step === 0 ? (page.input.kind === "file" ? page.file !== "" : origin.text.trim() !== "") : page.step === 1 ? !!page.state.preview && page.selected.length > 0 : page.step === 2 ? page.state.phase === "ready" && JSON.stringify(page.selected.slice().sort()) === JSON.stringify((page.state.targets || []).slice().sort()) : true)
            onClicked: {
                if (page.step === 0) page.send("preview", {input: inputType.value, text: origin.text, file: page.input.kind === "file" ? page.file : undefined, choice: choice.value, token: page.input.private_token ? token.text : undefined, station: Number(station.value)})
                else if (page.step === 1) page.send("match", {targets: page.selected, title: title.text, description: description.text, public: publicPlaylist.checked, folder: Number(folder.value)})
                else if (page.step === 2) page.send("commit", {title: title.text, description: description.text, public: publicPlaylist.checked, folder: Number(folder.value)})
                else { page.send("reset"); page.step = 0 }
            }
        }
        PillButton { text: I18n.tr("Cancel", I18n.rev); visible: page.busy; onClicked: page.send("cancel") }
    }

}
