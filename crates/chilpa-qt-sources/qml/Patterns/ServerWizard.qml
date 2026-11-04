import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/wizard/qml/Components"
import "../Components"

// Adding a music server one step at a time, over the numbered rail a
// plugin's setup draws (PluginSetupWizard): its kind, how to reach it, a
// test of the connection, its music, and a last look. Every step that goes
// to the network is the facade's (`Servers`); the wizard keeps the page and
// what was typed, and starts over on every open. Editing a server starts at
// its connection, its kind fixed. What was typed lives in the pages, which
// leave with the window: a password is never kept after it closes.
Modal {
    id: wizard
    property int serverId: -1
    property var preview: null
    readonly property var setup: preview || JSON.parse(Servers.setup || "{}")
    readonly property string step: setup.step || "idle"
    readonly property bool busy: setup.busy === true
    readonly property bool editing: serverId >= 0
    property string chosenKind: ""
    readonly property string kind: editing || preview ? (setup.kind || "") : chosenKind
    readonly property bool dlna: kind === "dlna"
    property int page: 0
    property bool sentSignIn: false
    // What the wizard says itself when it does not ask the server.
    property string said: ""
    readonly property string error: said !== "" ? said : setup.error || ""
    property var chosen: []
    property var names: ({})
    readonly property var pages: content.item
    readonly property bool connected: step === "libraries" && !busy
    readonly property bool failed: !busy && error !== "" && step !== "libraries" && step !== "done"
    readonly property bool done: step === "done"
    function show(id) { serverId = id; open() }
    ServerWizardWords { id: words; wizard: wizard }

    width: Math.min((parent ? parent.width : Metrics.serverWizardWidth) - Metrics.modalInsets, Metrics.serverWizardWidth)
    height: Math.min(Metrics.serverWizardHeight, (parent ? parent.height : Metrics.serverWizardHeight) - Metrics.modalInsets)

    onAboutToShow: {
        page = editing ? 1 : 0
        chosenKind = ""; sentSignIn = false; said = ""; chosen = []; names = ({})
        if (preview) return
        if (editing) {
            Servers.begin("edit/" + serverId)
            chosen = (setup.selectedLibraries || []).slice()
            const named = {}
            for (const folder of (setup.folders || [])) named[folder.id] = folder.name
            names = named
        } else Servers.cancel()
    }
    onClosed: if (!preview) Servers.cancel()
    onStepChanged: advance()
    onBusyChanged: advance()
    Timer {
        interval: Motion.poll
        repeat: true
        running: wizard.visible && !wizard.preview && (wizard.busy || wizard.step === "code")
        onTriggered: Servers.refresh()
    }

    // The facade answered: a server that asks for its sign-in gets what was
    // typed, once per test; an edit that was saved closes.
    // It reads the step itself: a property bound to it may not have caught
    // up when this runs.
    function advance() {
        const now = setup
        if (preview || !visible) return
        if (now.step === "done" && editing) return close()
        if (page !== 2 || now.busy === true || now.step !== "signin" || sentSignIn || now.error) return
        const typed = pages.connect
        sentSignIn = true
        if (kind === "jellyfin" && typed.quick && now.quickConnect !== true)
            said = I18n.tr("Quick Connect is off on this server. Go back and sign in with your user and password, or turn Quick Connect on in its dashboard.", I18n.rev)
        else if (kind === "jellyfin" && typed.quick) Servers.quickConnect()
        else if (kind === "subsonic" && typed.useKey && now.apiKey !== true)
            said = I18n.tr("This server does not take an API key. Go back and sign in with your user and password.", I18n.rev)
        else Servers.signIn(typed.user, typed.secret, kind === "subsonic" && typed.useKey)
    }
    function test() {
        sentSignIn = false; said = ""
        if (!editing) { chosen = []; names = ({}) }
        const typed = pages.connect
        page = 2
        if (kind === "plex" && !editing) { if (failed || step === "idle") Servers.begin("plex"); return }
        if (editing && typed.changeSignIn) Servers.begin("reauth")
        if (kind === "plex" && editing && typed.changeSignIn) return
        Servers.address(typed.address)
    }
    function primary() {
        if (page === 0) { Servers.begin(chosenKind); page = 1 }
        else if (page === 1 || (page === 2 && failed)) test()
        else if (page === 2) {
            if (!dlna && chosen.length === 0) chosen = (setup.choices || []).map(choice => choice.id)
            page = 3
        } else if (page === 3) page = 4
        else if (done) close()
        else Servers.finish(JSON.stringify({label: pages.summary.label, libraries: chosen}))
    }
    function back() {
        said = ""
        if (page === 1 && !editing) { Servers.cancel(); page = 0 }
        else page -= 1
    }
    function pick(id, name, on) {
        const rest = chosen.filter(one => one !== id)
        chosen = on ? rest.concat([id]) : rest
        const named = Object.assign({}, names)
        if (on) named[id] = name
        names = named
    }
    readonly property bool primaryEnabled: !preview && (page === 0 ? chosenKind !== ""
        : page === 1 ? !!pages && pages.connect.ready && (!busy || setup.searching === true)
        : page === 2 ? failed || connected
        : page === 3 ? chosen.length > 0 && !busy
        : !busy)
    readonly property string primaryText: page === 0 || page === 3 || (page === 2 && !failed) ? I18n.tr("Next", I18n.rev)
        : page === 1 ? (kind === "plex" && !editing ? I18n.tr("Sign in with Plex", I18n.rev) : I18n.tr("Test the connection", I18n.rev))
        : page === 2 ? I18n.tr("Try again", I18n.rev)
        : done ? I18n.tr("Close", I18n.rev) : editing ? I18n.tr("Save changes", I18n.rev) : I18n.tr("Add server", I18n.rev)
    readonly property var labels: [I18n.tr("Server type", I18n.rev), I18n.tr("Connect", I18n.rev), I18n.tr("Test", I18n.rev),
        dlna ? I18n.tr("Music folder", I18n.rev) : I18n.tr("Libraries", I18n.rev), I18n.tr("Confirm", I18n.rev)]
    readonly property var document: ({
        step: page + 1,
        title: words.title,
        description: words.description,
        steps: labels.map((label, at) => ({label: label,
            state: at < page || (at === 0 && editing) || (done && at === 4) ? "complete" : at === page ? "current" : "upcoming"}))
    })

    Item {
        width: wizard.width - Metrics.modalInsets
        height: wizard.height - Metrics.modalInsets

        PluginSetupWizard {
            id: rail
            width: parent.width
            document: wizard.document
        }
        Flickable {
            id: flick
            objectName: "serverFormViewport"
            anchors.top: rail.bottom
            anchors.topMargin: Metrics.gap16
            anchors.bottom: tray.visible ? tray.top : footer.top
            anchors.bottomMargin: Metrics.gap12
            width: parent.width
            clip: true
            contentHeight: content.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            // Shown whenever the step is taller than its place, in the
            // margin beside it: nothing is hidden without a sign of it.
            ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.gap8; policy: ScrollBar.AlwaysOn }
            Loader {
                id: content
                width: flick.width
                active: wizard.visible
                sourceComponent: pagesComponent
            }
        }
        ServerWizardFooter { id: footer; anchors.bottom: parent.bottom; width: parent.width; wizard: wizard }
        // What comes in stays in sight while a media server's folders are
        // browsed; the folders scroll above it.
        ServerChosen {
            id: tray
            visible: wizard.page === 3 && wizard.dlna
            anchors.bottom: footer.top
            anchors.bottomMargin: Metrics.gap12
            width: parent.width
            chosen: wizard.chosen
            names: wizard.names
            rows: wizard.height < Metrics.serverWizardHeight ? 2 : 3 // derived: fewer rows when the dialog is short
            onPicked: (id, name, on) => wizard.pick(id, name, on)
        }
    }

    Component {
        id: pagesComponent
        Column {
            property alias connect: connectPage
            property alias summary: summaryPage
            width: content.width
            spacing: Metrics.gap12
            ServerKindChoice {
                visible: wizard.page === 0
                width: parent.width
                chosen: wizard.chosenKind
                onPicked: (kind) => wizard.chosenKind = kind
            }
            ServerConnect {
                id: connectPage
                visible: wizard.page === 1
                width: parent.width
                kind: wizard.kind
                setup: wizard.setup
                editing: wizard.editing
                preview: !!wizard.preview
                onSubmitted: if (wizard.primaryEnabled) wizard.primary()
            }
            ServerCheck {
                visible: wizard.page === 2
                width: parent.width
                kind: wizard.kind
                setup: wizard.setup
                connected: wizard.connected
                failed: wizard.failed
                error: wizard.error
            }
            ServerFolders {
                visible: wizard.page === 3 && wizard.dlna
                width: parent.width
                setup: wizard.setup
                chosen: wizard.chosen
                preview: !!wizard.preview
                onPicked: (id, name, on) => wizard.pick(id, name, on)
            }
            ServerLibraries {
                visible: wizard.page === 3 && !wizard.dlna
                width: parent.width
                choices: wizard.setup.choices || []
                chosen: wizard.chosen
                onPicked: (id, name, on) => wizard.pick(id, name, on)
            }
            ServerSummary {
                id: summaryPage
                visible: wizard.page === 4
                width: parent.width
                kind: wizard.kind
                setup: wizard.setup
                done: wizard.done
                comesIn: wizard.dlna ? wizard.chosen.map(id => wizard.names[id] || id)
                    : (wizard.setup.choices || []).filter(choice => wizard.chosen.indexOf(choice.id) >= 0).map(choice => choice.name)
            }
            // What the step advises (ServerWizardWords): under a media
            // server's folders, views that repeat the same songs; before a
            // first import, for every kind, how it goes.
            WizardBanner {
                objectName: "serverNote"
                visible: words.noteTitle !== ""
                variant: words.noteVariant
                title: words.noteTitle
                body: words.noteBody
            }
            // What went wrong after the test: a folder that did not open, a
            // server that could not be added.
            Text {
                objectName: "serverError"
                visible: wizard.page >= 3 && !wizard.busy && (wizard.setup.error || "") !== ""
                width: parent.width
                topPadding: Metrics.gap8
                text: wizard.setup.error || ""
                wrapMode: Text.WordWrap
                font.pointSize: Type.small
                color: Theme.color.statusNegative
            }
        }
    }
}
