import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/qml/Screens"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components" as Tools

// The library's screens, Home and the search results say their titles,
// counts, tools, column names and chips in each language without running
// into each other, in a typical window and the narrowest one, also a tenth
// wider than Inter sets them. Every sort and grouping shows its longest
// choice, as a listener can leave it.
TestCase {
    id: test
    name: "LibraryTextFit"
    when: windowShown && TextAudit.ready
    visible: true
    width: 1400
    height: 900

    // A typical page and the narrowest: a 960 window less the open sidebar.
    readonly property var widths: [1300, 748]

    Component { id: libraryPage; LibraryScreen {} }
    Component { id: albumsPage; AlbumsScreen {} }
    Component { id: artistsPage; ArtistsScreen {} }
    Component { id: tracksPage; TracksScreen {} }
    Component { id: homePage; HomeScreen {} }
    Component { id: pageHero; PageHero {} }
    Component { id: albumHero; AlbumHero {} }
    Component { id: crumbs; SourceCrumbs {} }
    Component { id: flyoutFactory; SearchFlyout { room: 700 } }
    SearchModel { id: hits }
    AlbumDetailModel { id: detail }
    Component {
        id: trackHeader
        TrackHeader {
            sortable: true
            cols: Tools.TrackColumns { width: 692; showSource: true; showPlays: true; showRating: true }
        }
    }

    function cleanup() {
        I18n.choose("en")
        SettingsModel.libraryView = "grid"
        Library.albumless = false
        Library.busy = false
        Library.summary = ""
        Library.order = "artist"
        Library.albumOrder = "artist"
        for (const key of ["ui.library.group", "ui.albums.group", "ui.tracks.group", "ui.tracks.order"])
            SettingsModel.keep(key, "")
    }

    // What the bridge writes: "{} albums · {} songs", and a track list's
    // count and length.
    function librarySummary() {
        return I18n.trCount("{} album", "{} albums", 1234) + " · " + I18n.trCount("{} song", "{} songs", 18432)
    }
    function trackSummary() {
        return I18n.trCount("{} song", "{} songs", 18432) + " · 1274 h 12 min"
    }

    // Six tracks, the first an hour long: its time is the widest a row says.
    function fillTracks(model) {
        model.clear()
        for (let i = 0; i < 6; ++i)
            model.append({ trackId: i + 1, rowKind: 0, number: i + 1, title: "Track " + i, artist: "Artist", album: "Record",
                sourceName: "", sourceInfo: "{}", composer: "", duration: i === 0 ? "1:02:47" : "3:00", plays: 2, quality: "FLAC", tier: 2,
                available: true, playing: false, selected: false, headerLabel: "", headerMeta: "", art: "", group: "" })
        model.summary = trackSummary()
    }

    function find(root, name) {
        const out = []
        const walk = (item) => { for (const child of item.children || []) { if (child.objectName === name) out.push(child); walk(child) } }
        walk(root)
        return out
    }
    function sortButtons(root) {
        const out = []
        TextAudit.walk(root, (item) => { if ("directed" in item && "options" in item && "turnable" in item) out.push(item) })
        return out
    }
    // Every sort and grouping set to its longest choice, through the
    // screen's own handler, so the tools are as wide as a listener makes them.
    function longest(root) {
        for (const button of sortButtons(root)) {
            const options = button.options
            let best = options[0]
            for (const option of options) if (option.text.length > best.text.length) best = option
            if (best && best.value !== button.value) button.picked(best.value)
        }
    }

    // A switch draws no text, so the audit cannot see a label over one: each
    // label beside a switch must end before the switch starts.
    function besideSwitches(root) {
        const out = []
        TextAudit.walk(root, (toggle) => {
            if (!("knobInset" in toggle) || !TextAudit.shown(toggle)) return
            const left = toggle.mapToItem(root, 0, 0).x
            for (const sibling of toggle.parent.children) {
                if (!TextAudit.isText(sibling) || sibling.text === "" || !TextAudit.shown(sibling)) continue
                const box = TextAudit.painted(sibling)
                const at = sibling.mapToItem(root, box.x, 0).x
                if (at < left && at + box.width > left + TextAudit.slack)
                    out.push("\"" + sibling.text + "\" runs " + Math.ceil(at + box.width - left) + " px under its switch")
            }
        })
        return out
    }

    // Findings across every variant a case builds, said together.
    property var found: []
    function audit(root, data, where) {
        const more = TextAudit.check(test, root, data.scale).concat(besideSwitches(root))
        if (more.length > 0) found = found.concat([where + ":"]).concat(more)
    }
    function begin(data) { found = []; I18n.choose(data.language) }
    function end() { compare(found.length, 0, found.join("\n")) }

    function test_library_data() { return TextAudit.cases() }
    function test_library(data) {
        begin(data)
        for (const width of widths) {
            for (const view of ["grid", "tracks", "nerdy", "albumless"]) {
                Library.summary = librarySummary()
                SettingsModel.libraryView = view === "albumless" ? "grid" : view
                const page = createTemporaryObject(libraryPage, test, { width: width, height: 860 })
                longest(page)
                find(page, "libraryTrackSummary")[0].parent.tracks.summary = trackSummary()
                if (view === "albumless") {
                    Library.showAlbumless(true)
                    tryVerify(() => find(page, "albumlessTracks").length === 1, 2000)
                    fillTracks(find(page, "albumlessTracks")[0].model)
                }
                audit(page, data, view + " at " + width)
                Library.albumless = false
                page.destroy()
                wait(0)
            }
        }
        end()
    }

    function test_menus_and_columns_data() { return TextAudit.cases() }
    function test_menus_and_columns(data) {
        begin(data)
        for (const view of ["tracks", "nerdy"]) {
            SettingsModel.libraryView = view
            const page = createTemporaryObject(libraryPage, test, { width: 748, height: 860 })
            if (view === "tracks") {
                for (const button of sortButtons(page)) {
                    const menu = findChild(button, "sortMenu")
                    menu.open()
                    tryVerify(() => menu.opened, 2000)
                    audit(menu.contentItem, data, "the menu that opens on " + button.options[0].text)
                    menu.close()
                    tryVerify(() => !menu.visible, 2000)
                }
            }
            const gear = find(page, "listColumnsButton")[0]
            mouseClick(gear)
            const chooser = findChild(gear, "albumColumns")
            tryVerify(() => chooser.opened, 2000)
            audit(chooser.contentItem, data, "the " + view + " column chooser")
            chooser.close()
            tryVerify(() => !chooser.visible, 2000)
            page.destroy()
            wait(0)
        }
        end()
    }

    function test_other_screens_data() { return TextAudit.cases() }
    function test_other_screens(data) {
        begin(data)
        for (const width of widths) {
            for (const factory of [albumsPage, artistsPage, tracksPage]) {
                const page = createTemporaryObject(factory, test, { width: width, height: 860 })
                longest(page)
                if (factory === tracksPage) fillTracks(page.scrollView.model)
                audit(page, data, String(page).replace(/_QML.*/, "") + " at " + width)
                page.destroy()
                wait(0)
            }
        }
        end()
    }

    function test_heroes_data() { return TextAudit.cases() }
    function test_heroes(data) {
        begin(data)
        detail.title = "Record"
        detail.kindLine = I18n.tr("Album", I18n.rev).toUpperCase() + " · 1969"
        detail.meta = I18n.trCount("{} song", "{} songs", 12) + " · 54 min · FLAC 24/96"
        detail.selectedId = 3
        for (const width of widths) {
            const playlist = createTemporaryObject(pageHero, test, { width: width, label: I18n.tr("Playlist", I18n.rev), name: "Playlist",
                meta: I18n.trCount("{} song", "{} songs", 120) + " · 8 h 2 min", canQueue: true, canSelect: true, markKind: "playlist", markKey: "1" })
            audit(playlist, data, "a playlist's hero at " + width)
            playlist.destroy()
            const record = createTemporaryObject(albumHero, test, { width: width, detail: detail })
            audit(record, data, "a record's hero at " + width)
            record.destroy()
            // Down a tree, the way back; at a root narrowed by columns (no
            // way back, as the bridge has it), the picks and Chilpa's folders.
            const down = createTemporaryObject(crumbs, test, { width: width, height: 40, view: {
                name: "Server", crumbs: ["Music", "Jazz", "Records"], explorer: false } })
            audit(down, data, "a source's way back at " + width)
            down.destroy()
            const root = createTemporaryObject(crumbs, test, { width: width, height: 40, view: {
                name: "Radio", crumbs: [], explorer: true,
                columns: [{ id: "genre", title: I18n.tr("Genre", I18n.rev), picked: "jazz", pickedTitle: "Jazz" },
                          { id: "country", title: I18n.tr("Country", I18n.rev), picked: "mx", pickedTitle: "Mexico" }],
                shortcuts: [{ id: "chilpa:saved", title: I18n.tr("Saved", I18n.rev) },
                            { id: "chilpa:unfinished", title: I18n.tr("Continue listening", I18n.rev) }] } })
            audit(root, data, "a source's picks at " + width)
            root.destroy()
            wait(0)
        }
        end()
    }

    // A track list's fixed columns are as wide as their names: the header
    // says "Duración" whole, not "Dura…". Set in the face the columns
    // measure with, the window's own. The title, artist and album columns
    // share what is left, as their rows do.
    function test_track_header_names_data() { return TextAudit.cases().filter(c => c.scale === 1) }
    function test_track_header_names(data) {
        I18n.choose(data.language)
        for (const rich of [false, true]) {
            const head = createTemporaryObject(trackHeader, test, { y: 0 })
            head.cols.rich = rich
            const fixed = ["source", "quality", "rating", "plays", "time"].map(key => head.cols.names[key])
            for (const order of ["plays", "duration"]) {
                head.order = order
                wait(0)
                for (const name of head.children) {
                    if (!TextAudit.isText(name) || !name.visible || fixed.indexOf(name.text) < 0) continue
                    verify(!name.truncated, name.text + " is whole in " + Math.round(name.width) + " px, " + (rich ? "rich" : "compact") + ", sorted by " + order)
                }
            }
            head.destroy()
        }
    }

    function test_home_data() { return TextAudit.cases() }
    function test_home(data) {
        begin(data)
        for (const width of widths) {
            const page = createTemporaryObject(homePage, test, { width: width, height: 860 })
            tryCompare(page, "presented", true)
            audit(page, data, "Home at " + width)
            page.destroy()
            wait(0)
        }
        end()
    }

    function test_search_data() { return TextAudit.cases() }
    function test_search(data) {
        begin(data)
        const model = hits
        model.clear()
        const row = (kind, title, detail, duration, sourceName) => ({ kind: kind, title: title, detail: detail, art: "", duration: duration,
            sourceIcon: "", sourceName: sourceName, aiGenerated: false })
        model.append(row(0, I18n.tr("Top result", I18n.rev), "", "", ""))
        model.append(row(1, "Artist", I18n.tr("Artist", I18n.rev), "", ""))
        model.append(row(0, I18n.tr("Songs", I18n.rev), "", "", ""))
        model.append(row(4, "Song", "Artist · Record", "4:58", "A source"))
        model.append(row(5, "", "More results", "", ""))
        model.text = "song"
        const flyout = createTemporaryObject(flyoutFactory, test, { search: model })
        flyout.open()
        tryVerify(() => flyout.opened, 2000)
        audit(flyout.contentItem, data, "the search flyout")
        flyout.close()
        end()
    }
}
