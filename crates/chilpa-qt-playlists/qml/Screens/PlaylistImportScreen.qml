import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/wizard/qml/Components"
import "../Patterns"
import "../Theme"

// The playlist importer in four steps: where the list comes from, where it
// goes, its matches, and what was made. The plugin owns the work; the page
// keeps the step, asks for the status only while something runs (ADR-018)
// and sends what each step gathered.
Item {
    id: page
    readonly property var document: JSON.parse(PlaylistImport.snapshot || "{}")
    readonly property var state: document.state || ({})
    readonly property var sources: document.sources || []
    property int step: 0
    property string filter: "all"
    property int offset: 0
    readonly property bool busy: !!state.busy || !!document.pending
    // The last command this page sent, so a wait says what it waits for.
    property string sent: ""
    readonly property bool reading: step === 0 && busy && (sent === "preview" || state.phase === "fetching")
    property string previousPhase: ""
    // Every place a song can come from or go to, by id, as the steps name it.
    readonly property var places: {
        const all = {mixed: {name: I18n.tr("Chilpa playlist", I18n.rev), detail: I18n.tr("Best quality from any connected source, kept in Chilpa", I18n.rev), info: {chilpa: true}},
            local: {name: I18n.tr("Your library", I18n.rev), detail: I18n.tr("Only songs already in your library", I18n.rev), info: {local: true}}}
        for (const source of sources)
            all[source.id] = {name: source.name, detail: I18n.trArgs("Playlist in your {} account", [source.name], I18n.rev), info: {icon: source.icon || ""}, writable: !!source.writable}
        return all
    }
    readonly property var titles: [I18n.tr("Where is the playlist?", I18n.rev), I18n.tr("Where should it go?", I18n.rev),
        I18n.tr("Review the matches", I18n.rev), I18n.tr("Import complete", I18n.rev)]
    readonly property var labels: [I18n.tr("Origin", I18n.rev), I18n.tr("Destinations", I18n.rev), I18n.tr("Matches", I18n.rev), I18n.tr("Done", I18n.rev)]
    function send(action, payload) { sent = action; PlaylistImport.command(action, JSON.stringify(payload || {})) }
    function ask(filter, offset) { page.filter = filter; page.offset = offset; send("status", {filter: filter, offset: offset}) }
    function openPlaylist(id) {
        const window = page.Window.window
        if (typeof window?.deferNavigation === "function") window.deferNavigation(Playlists.open, id)
        else Playlists.open(id)
    }
    function openPart(output, part) {
        if (output.target === "local" || output.target === "mixed") openPlaylist(Number(part.ref))
        else Navigation.goKey("source", output.target + " native:" + part.ref)
    }
    onStateChanged: {
        if (state.phase === "done" && previousPhase !== "done") ask("unmatched", 0)
        if (state.phase !== previousPhase) {
            if (state.phase === "preview") step = 1
            if (["matching", "ready", "creating", "adding"].indexOf(state.phase) >= 0) step = 2
            if (state.phase === "done") step = 3
            if (state.phase === "input") step = 0
            if (state.phase === "cancelled") step = state.preview ? 1 : 0
        }
        previousPhase = state.phase || ""
    }
    Component.onCompleted: PlaylistImport.refresh()
    // Only while something runs: a page at rest asks the plugin nothing.
    Timer { objectName: "importPoll"; interval: 500; running: page.visible && page.busy; repeat: true; onTriggered: PlaylistImport.refresh() }
    Column {
        id: heading
        anchors.horizontalCenter: parent.horizontalCenter
        y: Metrics.contentTop
        width: Math.min(ImportFrame.column, page.width - Metrics.contentX * 2) // derived: page inset on both sides
        spacing: Metrics.gap16
        Text { text: I18n.tr("Import playlist", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.title; font.weight: Type.bold }
        Text { width: parent.width; visible: !page.document.installed; wrapMode: Text.Wrap; color: Theme.color.textSecondary; text: I18n.tr("Install and enable a playlist importer in Settings → Plugins.", I18n.rev) }
        PluginSetupWizard {
            objectName: "importSteps"
            width: parent.width; showDescription: false; visible: !!page.document.installed
            document: ({step: page.step + 1, title: page.titles[page.step], steps: page.labels.map((label, at) => ({label: label, state: at < page.step ? "complete" : "pending"}))})
        }
        Text { objectName: "importError"; width: parent.width; visible: text !== ""; wrapMode: Text.Wrap; color: Theme.color.statusNegative; font.pointSize: Type.small; text: page.document.error || page.state.error || "" }
    }
    Item {
        id: body
        x: heading.x; width: heading.width
        anchors.top: heading.bottom; anchors.topMargin: Metrics.gap24
        anchors.bottom: footer.top; anchors.bottomMargin: Metrics.gap16
        visible: !!page.document.installed
        ImportOriginStep {
            id: origin
            width: parent.width; visible: page.step === 0
            inputs: (page.document.options || {}).inputs || []
            choices: (page.document.choices || {}).choices || []
            reading: page.reading; busy: page.busy
            onFindChoices: payload => page.send("choices", payload)
        }
        ImportDestinationsStep {
            id: destinations
            anchors.fill: parent; visible: page.step === 1
            status: page.state; places: page.places; sources: page.sources; folders: page.document.folders || []; busy: page.busy
        }
        ImportReviewStep {
            objectName: "importReview"
            anchors.fill: parent; visible: page.step === 2
            status: page.state; places: page.places; filter: page.filter; offset: page.offset
            onAsked: (filter, offset) => page.ask(filter, offset)
            onChose: (index, target, source, reference) => page.send("choose", {index: index, target: target, source: source, ref: reference})
        }
        ImportSummaryStep {
            anchors.fill: parent; visible: page.step === 3
            status: page.state; places: page.places
            onOpened: (output, part) => page.openPart(output, part)
        }
    }
    Item {
        id: footer
        x: heading.x; width: heading.width; height: Metrics.pillHeight
        anchors.bottom: parent.bottom; anchors.bottomMargin: Metrics.contentBottom
        visible: !!page.document.installed
        PillButton {
            objectName: "importBack"
            anchors.left: parent.left
            visible: page.step > 0 && page.step < 3
            enabled: !page.busy
            text: I18n.tr("Back", I18n.rev); glyph: Lucide.circleArrowLeft; iconFamily: Lucide.font
            onClicked: page.step -= 1
        }
        Row {
            anchors.right: parent.right
            spacing: Metrics.gap8
            PillButton { objectName: "importCancel"; visible: page.busy; text: I18n.tr("Cancel", I18n.rev); onClicked: page.send("cancel") }
            PillButton {
                objectName: "importPrimary"
                kind: "primary"
                text: [I18n.tr("Read playlist", I18n.rev), I18n.tr("Find matches", I18n.rev), I18n.tr("Create playlists", I18n.rev), I18n.tr("Import another", I18n.rev)][page.step]
                enabled: !page.busy && (page.step === 0 ? origin.ready : page.step === 1 ? destinations.ready
                    : page.step === 2 ? page.state.phase === "ready" && destinations.matchesTargets(page.state.targets || []) : true)
                onClicked: {
                    if (page.step === 0) page.send("preview", origin.payload())
                    else if (page.step === 1) page.send("match", destinations.payload())
                    else if (page.step === 2) page.send("commit", destinations.settingsPayload())
                    else { page.send("reset"); page.step = 0 }
                }
            }
        }
    }
}
