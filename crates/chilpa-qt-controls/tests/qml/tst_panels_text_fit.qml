import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components" as Kit
import "../../qml/Components"
import "../../qml/Patterns"

// The modals and panels of the window's controls say everything in each
// language without running out of their boxes or into each other, as Inter
// sets them and a tenth wider, in a roomy window and in the least one.
TestCase {
    id: test
    name: "PanelsTextFit"
    when: windowShown && TextAudit.ready
    visible: true
    width: 1100
    height: 850

    // Where a modal opens: the window as the test has it, and the least
    // window at an interface size of 1.5 (960 x 600 physical).
    Item { id: roomy; width: test.width; height: test.height }
    Item { id: least; width: 640; height: 400 }
    readonly property var hosts: [roomy, least]

    Component { id: audioPanel; AudioOutputPanel { preview: true } }
    Component { id: audioPages; Column { property var document: ({}); spacing: Metrics.gap12
        AudioNow { width: parent.width; document: parent.document.now || ({}) }
        AudioNow { width: parent.width; document: parent.document.cast || ({}) }
        AudioEquipment { width: parent.width; document: parent.document }
        AudioCheck { width: parent.width; document: parent.document } } }
    Component { id: sleepTimer; SleepTimer {} }
    Component { id: trackInfo; TrackInfoModal {} }
    Component { id: queueAbout; QueueAbout {} }
    Component { id: quitQuestion; QuitQuestion {} }
    Component { id: mergeDialog; QueueMergeDialog {} }
    Component { id: confirmation; Kit.ConfirmationDialog {} }
    Component { id: playlistRemoval; Kit.ModelActionConfirmation {} }
    Component { id: progress; Kit.QueueProgress {} }
    Component { id: footer; QueueFooter {} }
    Component { id: loopRow; LoopRow {} }
    Component { id: shortcutsSheet; ShortcutsSheet {} }
    Component { id: shortcutsEditor; ShortcutsEditor {} }
    Component { id: commandBar; CommandBar {} }

    function cleanup() {
        I18n.choose("en")
        Controls.sleepLeft = ""
        Controls.loopState = 0
        QueueActivity.busy = false
        NowAbout.trackAnswer = ({})
        NowAbout.albumAnswer = ({})
        Player.title = "Fixture track"
        Hotkeys.snapshot = "{}"
    }

    // Everything the audio panel can say at once, in the words the facade
    // sends in the chosen language.
    function audioDocument() {
        const t = text => I18n.tr(text)
        return {
            name: "USB Audio DAC Pro · Front Speakers", selection: "alsa:front", current: "alsa:default",
            canUse: true, canCheck: true, busy: true, checking: true, linuxDiagnostics: true, exclusiveRequested: true,
            outputs: [{id: "alsa:default", label: t("Default"), detail: ""},
                {id: "alsa:front", label: "USB Audio DAC Pro · Front Speakers", detail: "hw:CARD=Pro,DEV=0 · USB Audio Class 2.0"}],
            error: t("Equipment changed. Read its details again before checking a format."),
            status: t("Details available"),
            inspection: {render_available: false, identity: {strong: false, backend: "alsa", generation: "revision-1",
                stable_id: "usb-0d8c:0102 USB Audio DAC Pro serial 0000000042"}},
            attributes: [{name: t("Input rates (Hz)"), value: "44100, 48000, 88200, 96000, 176400, 192000, 352800, 384000",
                scope: t("Convertible system input; physical DAC capabilities are unknown")},
                {name: t("Input sample formats"), value: "PCM 16, PCM 24, PCM 24/32, PCM 32, Float 32, DSD U32 BE", scope: t("Declared by the hardware driver")}],
            formats: [{description: "PCM · 44100–384000 Hz · 16/24/32 bits · 2 channels", scope: t("Driver query; playback has not been tested")}],
            history: [{requested: "352800 Hz · PCM 24/32 · 2 channels", status: t("Permission required"), audible: t("Untested"), sameContext: false,
                case: {timestamp: 1793000000, error: t("Some endpoint streams could not be read.")}}],
            last: {error: t("Some endpoint streams could not be read.")},
            lastRequested: "352800 Hz · PCM 24/32 · 2 channels", lastOpened: "", lastStatus: t("Different format"),
            lastPhysicalStatus: t("Permission required"), saved: false,
            now: {available: true, exclusiveObserved: null, exclusiveRequested: true,
                summary: t("No sample changes are reported in Chilpa; the downstream path is unverified."),
                source: "FLAC · 352800 Hz · 24 musical bits", processing: t("Samples converted to floating point") + "\n" + t("Gain applied to consumed PCM blocks"),
                output: "352800 Hz · PCM 24/32 · 2 channels", readback: t("No physical readback is available for this route."),
                readbackScope: t("Read at opening on the selected endpoint. This does not measure audible output.")},
            cast: {available: true, cast: true, summary: t("Chilpa sends the original audio to the receiver."), rendererStatus: t("Busy"),
                rendererError: t("The receiver does not report its physical DAC format."), source: "FLAC · 44100 Hz",
                processing: t("Local DSP and software volume are bypassed."), output: t("Original audio over the selected network transport"),
                readback: t("The receiver does not report its physical DAC format.")}
        }
    }

    // What is known of a long classical track, with its file gone.
    function trackAnswer() {
        const t = text => I18n.tr(text)
        return {loading: false, origin: "MusicBrainz", link: "https://musicbrainz.org/recording/1",
            description: "Recorded live at the Großer Musikvereinssaal over three evenings in a single take per movement.",
            facts: [{label: t("Composer"), value: "Pyotr Ilyich Tchaikovsky"},
                {label: t("Work"), value: "Symphony No. 6 in B minor, Op. 74 “Pathétique”: I. Adagio – Allegro non troppo"},
                {label: t("Genre"), value: "Classical, Romantic, Orchestral"}, {label: t("Release date"), value: "1960-09-01"},
                {label: t("Label"), value: "Deutsche Grammophon"}, {label: t("Catalog number"), value: "DG 2530 774"},
                {label: t("Copyright"), value: "℗ 1960 Deutsche Grammophon GmbH, Hamburg"}],
            credits: [{name: "Berliner Philharmoniker", roles: "Orchestra"},
                {name: "Herbert von Karajan und das Orchester der Deutschen Oper Berlin", roles: "Conductor, recording producer, liner notes"}],
            file: {title: t("File"), note: t("Not found on disk"),
                value: "/run/media/listener/Archive/Classical/Tchaikovsky/Symphony No. 6 (Karajan, 1960)/01 - I. Adagio – Allegro non troppo.flac"}}
    }

    function audit(data, root, what) {
        return TextAudit.check(test, root, data.scale).map(line => what + ": " + line)
    }
    function show(factory, host, properties) {
        const modal = createTemporaryObject(factory, test, Object.assign({parent: host}, properties || {}))
        verify(modal)
        modal.open()
        tryCompare(modal, "opened", true)
        return modal
    }
    function settle(item) {
        waitForPolish(item)
        wait(0)
    }

    function test_audio_panel_data() { return TextAudit.cases() }
    function test_audio_panel(data) {
        I18n.choose(data.language)
        let found = []
        for (const host of hosts) for (const page of ["now", "equipment", "check"]) {
            const panel = show(audioPanel, host, {previewDocument: audioDocument()})
            panel.page = page
            settle(panel.contentItem)
            found = found.concat(audit(data, panel.contentItem, "panel " + page + " " + host.width))
            panel.close()
        }
        compare(found.length, 0, found.join("\n"))
    }

    // The panel's pages scroll; whole, they are audited at the room the
    // panel gives them in each window.
    function test_audio_pages_data() { return TextAudit.cases() }
    function test_audio_pages(data) {
        I18n.choose(data.language)
        let found = []
        for (const host of hosts) {
            const wide = Math.min(Metrics.outputDetailsWidth + Metrics.modalInsets, host.width - Metrics.modalInsets) - Metrics.modalInsets
            const pages = createTemporaryObject(audioPages, test, {width: wide, document: audioDocument()})
            settle(pages)
            found = found.concat(audit(data, pages, "pages " + wide))
            pages.destroy()
        }
        compare(found.length, 0, found.join("\n"))
    }

    function test_sleep_timer_data() { return TextAudit.cases() }
    function test_sleep_timer(data) {
        I18n.choose(data.language)
        let found = []
        for (const left of ["", "1:59:58"]) {
            Controls.sleepLeft = left
            const timer = show(sleepTimer, roomy)
            timer.chosen = timer.minutes.length
            settle(timer.contentItem)
            found = found.concat(audit(data, timer.contentItem, left === "" ? "timer off" : "timer running"))
            timer.close()
        }
        compare(found.length, 0, found.join("\n"))
    }

    function test_track_info_data() { return TextAudit.cases() }
    function test_track_info(data) {
        I18n.choose(data.language)
        Player.title = "Symphony No. 6 in B minor, Op. 74 “Pathétique”: I. Adagio – Allegro non troppo"
        NowAbout.trackAnswer = trackAnswer()
        NowAbout.albumAnswer = Object.assign(trackAnswer(), {can_refetch: true, file: null})
        let found = []
        for (const host of hosts) for (const part of ["track", "album"]) {
            const modal = show(trackInfo, host)
            findChild(modal, "trackInfoAbout").part = part
            settle(modal.contentItem)
            found = found.concat(audit(data, modal.contentItem, "track info " + part + " " + host.width))
            modal.close()
        }
        compare(found.length, 0, found.join("\n"))
    }

    // Whole, at the modal's room and the queue page's column.
    function test_track_facts_data() { return TextAudit.cases() }
    function test_track_facts(data) {
        I18n.choose(data.language)
        NowAbout.trackAnswer = trackAnswer()
        NowAbout.albumAnswer = Object.assign(trackAnswer(), {can_refetch: true, failed: true, file: null})
        let found = []
        for (const wide of [Metrics.trackInfoWidth, Metrics.listenArt]) for (const part of ["track", "album"]) {
            const about = createTemporaryObject(queueAbout, test, {width: wide, height: 2000, part: part})
            settle(about)
            found = found.concat(audit(data, about, "facts " + part + " " + wide))
            about.destroy()
        }
        compare(found.length, 0, found.join("\n"))
    }

    function test_quit_question_data() { return TextAudit.cases() }
    function test_quit_question(data) {
        I18n.choose(data.language)
        const question = createTemporaryObject(quitQuestion, test, {parent: least})
        question.ask()
        tryCompare(question, "opened", true)
        settle(question.contentItem)
        const found = audit(data, question.contentItem, "quit")
        compare(found.length, 0, found.join("\n"))
    }

    function test_queue_merge_data() { return TextAudit.cases() }
    function test_queue_merge(data) {
        I18n.choose(data.language)
        let found = []
        for (const host of hosts) {
            const dialog = createTemporaryObject(mergeDialog, test, {parent: host, document: {selected: "1", tabs: [
                {id: "1", name: "Sunday morning in the garden with coffee", count: 128}, {id: "2", name: "Road trip", count: 42},
                {id: "3", name: "Late night", count: 7}]}})
            dialog.show()
            tryCompare(dialog, "opened", true)
            dialog.error = I18n.tr("Choose queues and arrange their order. Repeated tracks are kept.")
            settle(dialog.contentItem)
            // In the least window the body is taller than the modal and the
            // modal scrolls over it: the body is audited whole, and every
            // part of it can be scrolled to.
            const body = Array.from(dialog.body).find(part => part instanceof Item)
            verify(dialog.contentItem.contentHeight >= body.implicitHeight + Metrics.modalInsets, "the foot of the dialog cannot be scrolled to")
            found = found.concat(audit(data, body, "merge " + host.width))
            dialog.close()
        }
        compare(found.length, 0, found.join("\n"))
    }

    function test_confirmations_data() { return TextAudit.cases() }
    function test_confirmations(data) {
        I18n.choose(data.language)
        let found = []
        const asked = show(confirmation, least, {destructive: true, doubleConfirmation: true,
            title: I18n.tr("Clear temporary remote images?"),
            message: I18n.tr("Images from sources and plugins will download again when needed. Artwork saved in your library stays."),
            confirmText: I18n.tr("Clear"), option: I18n.tr("Also delete the lyrics saved for this plugin's tracks"), targetValid: false})
        findChild(asked.contentItem, "confirmationAccept").clicked()
        settle(asked.contentItem)
        found = found.concat(audit(data, asked.contentItem, "confirmation"))
        asked.close()
        const removal = createTemporaryObject(playlistRemoval, test, {parent: least})
        removal.open()
        tryCompare(removal, "opened", true)
        settle(removal.contentItem)
        found = found.concat(audit(data, removal.contentItem, "playlist removal"))
        compare(found.length, 0, found.join("\n"))
    }

    // The foot of the side queue, at the panel's own width and its least,
    // searching; and the note that a long list is being added.
    function test_queue_foot_data() { return TextAudit.cases() }
    function test_queue_foot(data) {
        I18n.choose(data.language)
        let found = []
        for (const wide of [Metrics.rightPanel, 212]) {
            const foot = createTemporaryObject(footer, test, {width: wide, height: Metrics.panelHeader})
            foot.openSearch()
            tryCompare(findChild(foot, "queueSearch"), "width", wide - 2 * Metrics.gap6)
            settle(foot)
            found = found.concat(audit(data, foot, "foot " + wide))
            foot.destroy()
        }
        QueueActivity.busy = true
        const note = createTemporaryObject(progress, test)
        settle(note)
        found = found.concat(audit(data, note, "adding"))
        compare(found.length, 0, found.join("\n"))
    }

    // The keys as the facade sends them: the window's groups and actions with
    // their default keys, one of them being given another that is taken.
    function shortcuts() {
        const t = text => I18n.tr(text)
        const group = (label, rows) => ({label: t(label), rows: rows.map(([id, name, key]) => ({id: id, label: t(name), shortcut: key, modified: id === "nav.back"}))})
        return {keymap: "default", modified: 3, recording: "nav.back", tried: "Ctrl + Shift + ◀", takenBy: t("Seek Back"),
            col1: [group("Playback", [["playback.toggle", "Play / Pause", "Space"], ["playback.next", "Next Track", "Ctrl + ▶"],
                ["playback.prev", "Previous Track", "Ctrl + ◀"], ["playback.volumeUp", "Volume Up", "Ctrl + ▲"], ["playback.volumeDown", "Volume Down", "Ctrl + ▼"],
                ["playback.mute", "Mute / Unmute", "M"], ["playback.shuffle", "Toggle Shuffle", "Ctrl + S"], ["playback.repeat", "Cycle Repeat", "Ctrl + R"],
                ["playback.favorite", "Like Track", "Alt + Shift + B"], ["playback.seekForward", "Seek Forward", "Ctrl + Shift + ▶"],
                ["playback.seekBack", "Seek Back", "Ctrl + Shift + ◀"], ["playback.sleep", "Sleep timer", ""]])],
            col2: [group("Navigation", [["nav.back", "Go Back", "Alt + ◀"], ["nav.forward", "Go Forward", "Alt + ▶"], ["nav.search", "Search", "Ctrl + K"],
                ["nav.settings", "Settings", "Ctrl + ,"], ["nav.newTab", "New Tab", "Ctrl + T"], ["nav.closeTab", "Close Tab", "Ctrl + W"],
                ["nav.nextTab", "Next tab", "Ctrl + Tab"], ["nav.previousTab", "Previous tab", "Ctrl + Shift + Tab"],
                ["queue.next", "Next queue", "Ctrl + Alt + ▶"], ["queue.previous", "Previous queue", "Ctrl + Alt + ◀"], ["nav.quit", "Quit", "Ctrl + Q"]]),
                group("Interface", [["ui.sidebar", "Toggle Sidebar", "Shift + S"], ["ui.escape", "Close / Dismiss", "Esc"], ["ui.showShortcuts", "Show Shortcuts", "?"],
                ["ui.openLink", "Open Music Link", "Ctrl + L"], ["ui.miniPlayer", "Toggle Mini Player", "Shift + M"], ["ui.focusMode", "Toggle Immersive View", "Shift + I"]])],
            col3: [group("Mini player", [["mini.micro", "Micro view", "1"], ["mini.artwork", "Artwork view", "3"], ["mini.lyrics", "Lyrics view", "5"]]),
                group("Immersive view", [["focus.seekForward", "Seek Forward 5 s", "▶"], ["focus.seekBackLong", "Seek Back 10 s", "Shift + ◀"]])]}
    }
    // An action's name cut short hides what the key does: the sheet's and
    // the editor's names wrap instead.
    function cutNames(root) {
        const out = []
        TextAudit.walk(root, item => {
            if (TextAudit.isText(item) && item.truncated && TextAudit.shown(item)) out.push("\"" + item.text + "\" is cut short")
        })
        return out
    }

    function test_shortcuts_data() { return TextAudit.cases() }
    function test_shortcuts(data) {
        I18n.choose(data.language)
        Hotkeys.snapshot = JSON.stringify(shortcuts())
        let found = []
        for (const host of hosts) for (const factory of [shortcutsSheet, shortcutsEditor]) {
            const modal = show(factory, host)
            settle(modal.contentItem)
            const what = (factory === shortcutsSheet ? "sheet " : "editor ") + host.width
            found = found.concat(audit(data, modal.contentItem, what), cutNames(modal.contentItem).map(line => what + ": " + line))
            modal.close()
        }
        compare(found.length, 0, found.join("\n"))
    }

    // The console in the least window: its completions, a refused line and
    // the help it lists. The completions and the help are drawn above the
    // console's top edge on purpose (CommandBar.qml, anchors.bottom:
    // parent.top), so each is audited as a surface of its own and left out
    // of the console's.
    function consoleFits(data, bar, what) {
        const above = item => item.parent === bar && item.y < 0
        let found = TextAudit.check(test, bar, data.scale, {ignore: above}).map(line => what + ": " + line)
        for (const part of bar.children) if (above(part)) found = found.concat(audit(data, part, what + " above"))
        return found
    }
    function test_command_bar_data() { return TextAudit.cases() }
    function test_command_bar(data) {
        I18n.choose(data.language)
        CommandLine.choices = ["queue clear", "queue save", "seek", "sleep", "settings"]
        const bar = createTemporaryObject(commandBar, least, {width: least.width, y: least.height - Metrics.fieldHeight - Metrics.gap16})
        bar.show("s", null)
        settle(bar)
        let found = consoleFits(data, bar, "completions")
        bar.error = I18n.tr("Unknown target; use Tab to complete the source.")
        settle(bar)
        found = found.concat(consoleFits(data, bar, "refused"))
        CommandLine.answer = JSON.stringify({action: "commands.help", lines: ["Toggle Immersive View", "Previous search result", "Filter the library", "Set preference"]
            .map((label, at) => ({usage: [":focus", ":search previous", ":filter [query]", ":set <key> <value>"][at], description: I18n.tr(label)}))})
        bar.submit()
        settle(bar)
        found = found.concat(consoleFits(data, bar, "help"))
        CommandLine.answer = "{}"
        compare(found.length, 0, found.join("\n"))
    }

    // The A-B loop among the playback options, in each of its states.
    function test_loop_row_data() { return TextAudit.cases() }
    function test_loop_row(data) {
        I18n.choose(data.language)
        let found = []
        const row = createTemporaryObject(loopRow, test, {width: Metrics.playbackOptionsWidth})
        for (const state of [0, 1, 2]) {
            Controls.loopState = state
            settle(row)
            found = found.concat(audit(data, row, "loop " + state))
        }
        compare(found.length, 0, found.join("\n"))
    }
}
