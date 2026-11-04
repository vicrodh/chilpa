import QtQuick
import app.chilpa.kit

// What each step of adding a server is called and says under its name, by
// the step, the kind and how the test went, read from the wizard it names
// (ServerWizard), or from any object with the same properties.
QtObject {
    property var wizard: ({})
    readonly property int page: wizard.page || 0
    readonly property string kind: wizard.kind || ""
    readonly property string step: wizard.step || ""
    readonly property bool editing: wizard.editing === true
    readonly property bool failed: wizard.failed === true
    readonly property bool connected: wizard.connected === true
    readonly property bool done: wizard.done === true
    readonly property bool dlna: kind === "dlna"
    readonly property var setup: wizard.setup || ({})
    readonly property var chosen: wizard.chosen || []
    // Under a media server's folders: one that holds several views of a
    // library, or two views of one level, read the same songs again. The
    // choice stays the person's; this only advises.
    readonly property bool repeats: {
        if (!dlna || page !== 3) return false
        const roots = setup.viewRoots || []
        if (chosen.some(id => roots.indexOf(id) >= 0)) return true
        const here = (setup.choices || []).map(choice => choice.id)
        return setup.views === true && chosen.filter(id => here.indexOf(id) >= 0).length > 1
    }
    // A folder chosen before that holds no songs, said while editing.
    readonly property bool noSongsChosen: dlna && !done && page >= 1
        && chosen.some(id => (setup.noMusic || []).some(n => n.id === id))
    // Before a first import: how it goes.
    readonly property bool firstImport: page === 4 && !done && !editing
    readonly property string noteVariant: noSongsChosen ? "warning" : "info"
    readonly property string noteTitle: noSongsChosen ? I18n.tr("A chosen folder holds no songs", I18n.rev)
        : repeats ? I18n.tr("Several views of one library", I18n.rev)
        : firstImport ? I18n.tr("Albums appear as they are read", I18n.rev) : ""
    readonly property string noteBody: noSongsChosen
        ? I18n.tr("It is not read, and saving leaves it out. Take it out and choose the folder that holds your songs.", I18n.rev)
        : repeats
        ? I18n.tr("This folder repeats the same songs under several views. Pick one view (we suggest By Folder) so each song is read once.", I18n.rev)
        : firstImport ? I18n.tr("If this is your first music source, we suggest waiting until it finishes. What has appeared plays meanwhile.", I18n.rev) : ""
    readonly property string title: {
        if (page === 0) return I18n.tr("What kind of server is it?", I18n.rev)
        if (page === 1) return editing ? I18n.tr("Its connection", I18n.rev) : dlna ? I18n.tr("Which media server?", I18n.rev)
            : kind === "plex" ? I18n.tr("Sign in with your Plex account", I18n.rev) : I18n.tr("Where is it, and who signs in?", I18n.rev)
        if (page === 2) return failed ? I18n.tr("Could not connect", I18n.rev) : connected ? I18n.tr("Connected", I18n.rev)
            : step === "code" ? I18n.tr("Type the code", I18n.rev) : step === "servers" ? I18n.tr("Which server?", I18n.rev)
            : I18n.tr("Testing the connection", I18n.rev)
        if (page === 3) return dlna ? I18n.tr("Where is your music?", I18n.rev) : I18n.tr("Which libraries come in?", I18n.rev)
        return done ? I18n.tr("Server added", I18n.rev) : editing ? I18n.tr("Save the changes?", I18n.rev) : I18n.tr("Ready to add it", I18n.rev)
    }
    readonly property string description: {
        if (page === 0) return I18n.tr("Its music joins your library, listed with your own files. Nothing on the server changes.", I18n.rev)
        if (page === 1) return editing && dlna ? I18n.tr("Change the address if the server moved.", I18n.rev)
            : editing ? I18n.tr("Change the address if the server moved. Its sign-in stays as it is unless you change it.", I18n.rev)
            : dlna ? I18n.tr("These answered a search of this network. Choose yours, or type its address.", I18n.rev)
            : kind === "plex" ? I18n.tr("Chilpa asks Plex for a code; on the next step you type it at plex.tv/link.", I18n.rev)
            : I18n.tr("The address you open the server at in a browser, with its port, and the user Chilpa signs in as.", I18n.rev)
        if (page === 2) return failed || step === "code" ? "" : connected ? I18n.tr("The server answered, and Chilpa can read its music.", I18n.rev)
            : step === "servers" ? I18n.tr("Your Plex account reaches more than one server.", I18n.rev) : I18n.tr("This takes a few seconds.", I18n.rev)
        if (page === 3) return dlna ? I18n.tr("Open folders until you reach the one that holds your music, and choose it. A server shows the same music in several views: choose one, such as its folders or its albums.", I18n.rev)
            : I18n.tr("Their music joins yours. With all of them chosen, a library added later on the server comes in too.", I18n.rev)
        return done ? I18n.tr("Its music is being read; it appears in the library as it arrives.", I18n.rev)
            : editing ? I18n.tr("Its music is read again with what you changed.", I18n.rev) : I18n.tr("Check its name and what comes in.", I18n.rev)
    }
}
