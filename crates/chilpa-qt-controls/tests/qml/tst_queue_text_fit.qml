import QtQuick
import "qrc:/qt/qml/app/chilpa/controls/qml/Patterns"
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/controls/qml/Screens"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"

// The side queue and the queue page say every title, count and button in
// each language without running out of their boxes or into each other, as
// Inter sets them and a tenth wider: the side queue at its own width and its
// least, empty and full; the page wide and just wide enough for the player
// beside it, and at the room the least window leaves it.
TestCase {
    id: test
    name: "QueueTextFit"
    when: windowShown && TextAudit.ready
    visible: true
    width: 1500
    height: 900

    Component { id: sidePanel; QueuePanel { height: 700 } }
    Component { id: queuePage; ListenListScreen { height: 900 } }

    readonly property var titles: [
        "Symphony No. 6 in B minor, Op. 74 “Pathétique”: I. Adagio – Allegro non troppo",
        "Concerto pour piano et orchestre n° 2 en fa mineur, op. 21 : II. Larghetto",
        "Ein feste Burg ist unser Gott, Kantate zum Reformationsfest, BWV 80",
        "Short"]

    function cleanup() {
        I18n.choose("en")
        Player.title = "Fixture track"
        ListenList.snapshot = JSON.stringify({ selected: "1", active: "1", name: "Jazz", tabs: [{id:"1",name:"Jazz",count:3,playing:true},{id:"2",name:"Other",count:2,playing:false}], undo:false })
        ListenList.footing = JSON.stringify({ summary: "3 songs 1 hr 2 min", left_ms: 600000 })
        NowAbout.trackAnswer = ({})
        AudioOutput.pathAnswer = "{}"
        SettingsModel.keep("ui.queue.panel", "playing")
    }

    // A count and a length as the facade says them in the chosen language.
    function summary(count) { return I18n.trCount("{} song", "{} songs", count) + " · 8 h 32 min" }
    function fill(model, rows, duration) {
        model.clear()
        for (let index = 0; index < rows; ++index)
            model.append({trackId: index + 1, rowKind: 0, number: index + 1, title: titles[index % titles.length],
                artist: "Berliner Philharmoniker, Herbert von Karajan", album: "Tchaikovsky: The Symphonies", sourceName: "",
                duration: duration, plays: 0, quality: "FLAC 24/192", tier: 3, available: true, playing: index === 0,
                selected: false, headerLabel: "", headerMeta: "", art: ""})
    }
    function queueOf(panel) {
        for (const thing of panel.resources) if (thing.kind === "queue") return thing
        return null
    }
    // The rows the list has built, and each of them laid out.
    function rowsBuilt(list, wanted) {
        let built = 0
        const walk = (item) => { for (const child of item.children) { if (child.title !== undefined && child.duration !== undefined && child.visible) built++; walk(child) } }
        walk(list)
        return built >= wanted
    }

    function test_side_queue_data() { return TextAudit.cases() }
    function test_side_queue(data) {
        I18n.choose(data.language)
        let found = []
        for (const wide of [Metrics.rightPanel, 212]) for (const rows of [0, 6]) {
            const panel = createTemporaryObject(sidePanel, test, {width: wide})
            const queue = queueOf(panel)
            verify(queue)
            fill(queue, rows, "1:02:47")
            queue.summary = rows ? summary(rows) : ""
            if (rows) tryVerify(() => rowsBuilt(panel, rows), 2000)
            waitForPolish(panel)
            found = found.concat(TextAudit.check(test, panel, data.scale).map(line => "side " + wide + " " + rows + ": " + line))
            panel.destroy()
        }
        compare(found.length, 0, found.join("\n"))
    }

    function test_queue_page_data() { return TextAudit.cases() }
    function test_queue_page(data) {
        I18n.choose(data.language)
        Player.title = titles[0]
        ListenList.snapshot = JSON.stringify({selected: "1", active: "1", name: "Sunday morning", undo: true, tabs: [
            {id: "1", name: "Sunday morning in the garden", count: 128, playing: true}, {id: "2", name: "Road trip", count: 42, playing: false}]})
        ListenList.footing = JSON.stringify({summary: summary(128), left_ms: 30720000})
        NowAbout.trackAnswer = ({loading: false, facts: [{label: I18n.tr("Release date"), value: "1960-09-01"}, {label: I18n.tr("Catalog number"), value: "DG 2530 774"}],
            credits: [], file: {title: I18n.tr("File"), note: I18n.tr("Not found on disk"), value: "/run/media/listener/Archive/Classical/01 - Adagio.flac"}})
        AudioOutput.pathAnswer = JSON.stringify({status: "unverified", summary: I18n.tr("No sample changes are reported in Chilpa; the downstream path is unverified."), stages: [
            {label: I18n.tr("Source"), value: "FLAC · 24-bit / 192 kHz"}, {label: I18n.tr("Processing"), value: I18n.tr("No sample processing reported in Chilpa")},
            {label: I18n.tr("Output"), value: "192000 Hz · PCM 24/32 · 2 channels"}, {label: I18n.tr("Device"), value: "USB Audio DAC Pro · Front Speakers"}]})
        let found = []
        for (const wide of [test.width, 920, 640]) for (const tab of ["playing", "about"]) {
            SettingsModel.keep("ui.queue.panel", tab)
            const page = createTemporaryObject(queuePage, test, {width: wide})
            // Under an hour: the shared track list's time column (TrackRow)
            // does not hold "1:02:47"; that one is audited with the lists.
            const tracks = findChild(page, "queueTracks")
            fill(tracks, 6, "12:47")
            tryVerify(() => rowsBuilt(findChild(page, "queueList"), 6), 2000)
            waitForPolish(page)
            found = found.concat(TextAudit.check(test, page, data.scale).map(line => "page " + wide + " " + tab + ": " + line))
            page.destroy()
        }
        compare(found.length, 0, found.join("\n"))
    }
}
