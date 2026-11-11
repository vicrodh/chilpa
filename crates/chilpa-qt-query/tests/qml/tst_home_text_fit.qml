import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/sources/qml/Components"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"

// Home's pieces and a source's page headers say their titles, actions and
// choices in each language without running into each other, in a typical
// window and the narrowest one (960 wide, sidebar open), also a tenth wider
// than Inter sets them.
TestCase {
    id: test
    name: "HomeTextFit"
    when: windowShown && TextAudit.ready
    visible: true
    width: 1400
    height: 900

    // A typical page and the narrowest: a 960 window less the open sidebar.
    readonly property var widths: [1300, 748]
    readonly property string blurb: "A long biography that runs over several lines, so the header has a description to cut and a way to read the rest of it."

    readonly property var shelves: [
        { id: "library.recentlyPlayed", title: I18n.tr("Recently played albums in the library", I18n.rev), compact: true, enabled: true, mode: "compact" },
        { id: "everywhere.recentlyPlayed", title: I18n.tr("Recently played albums", I18n.rev), compact: true, enabled: true, mode: "cards" },
        { id: "library.recentPlaylists", title: I18n.tr("Recently played playlists in the library", I18n.rev), enabled: true },
        { id: "everywhere.recentPlaylists", title: I18n.tr("Recently played playlists", I18n.rev), enabled: true },
        { id: "library.recentlyAdded", title: I18n.tr("Recently added albums", I18n.rev), enabled: true, size: 12 },
        { id: "library.mostPlayed", title: I18n.tr("Most played tracks", I18n.rev), compact: true, enabled: true },
        { id: "source:fixture:recent", title: "Fixture new releases", sourceId: "fixture", sourceName: "Fixture", enabled: true },
        { id: "source:fixture:off", title: "Fixture staff picks", sourceId: "fixture", sourceName: "Fixture", enabled: false }
    ]

    Component { id: pageHeader; SourcePageHeader {} }
    Component { id: showHeader; MediaShowHeader {} }
    Component { id: artistHeader; ArtistHeader {} }
    Component { id: customizerFactory; SectionsCustomizer {} }
    Component { id: shelfFactory; SourceShelf {} }
    Component { id: columnChoices; LibraryColumnChoices {} }

    function cleanup() {
        I18n.choose("en")
        test.width = 1400
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

    function test_headers_data() { return TextAudit.cases() }
    function test_headers(data) {
        found = []
        I18n.choose(data.language)
        for (const width of widths) {
            const album = createTemporaryObject(pageHeader, test, { width: width, info: {
                kind: "album", plugin: "fixture", ref: "album:1", title: "Album", artist: "Artist", released: "1969",
                genre: "Jazz", description: test.blurb, quality_label: "FLAC 24/96", quality_tier: 3, radio_id: -1,
                refreshable: true, library_supported: true, in_library: false, favorite: false } })
            audit(album, data, "the album header at " + width)
            album.destroy()
            const artist = createTemporaryObject(artistHeader, test, { width: width, sceneAvailable: true, info: {
                title: "Artist", plugin: "fixture", description: test.blurb, roles: ["Composer", "Arranger"], radio_id: -1,
                links: [{ title: "Site", url: "https://example.org" }] } })
            audit(artist, data, "the artist header at " + width)
            artist.destroy()
            const show = createTemporaryObject(showHeader, test, { width: width, entry: {
                title: "Show", subtitle: "Host", description: test.blurb, facts: [{ value: "42 episodes" }] } })
            audit(show, data, "the show header at " + width)
            show.destroy()
            wait(0)
        }
        compare(found.length, 0, found.join("\n"))
    }

    // A source's shelf keeps its title clear of its link and chevrons, and
    // the Library's column switches name each column beside its switch.
    function test_shelves_and_columns_data() { return TextAudit.cases() }
    function test_shelves_and_columns(data) {
        found = []
        I18n.choose(data.language)
        for (const width of widths) {
            const rail = createTemporaryObject(shelfFactory, test, { width: width, oneRow: true, section: {
                title: "Recommended new releases from the artists you follow", key: "fixture native:discover:home|recent",
                more: "page:2", items: [{ ref: "album:1" }, { ref: "album:2" }] } })
            audit(rail, data, "a source's shelf at " + width)
            rail.destroy()
            wait(0)
        }
        const choices = createTemporaryObject(columnChoices, test, { width: Metrics.sourcesPopoverWidth / 2, selected: ["year", "rate"] }) // derived: the popover's column
        audit(choices, data, "the Library's column switches")
        compare(found.length, 0, found.join("\n"))
    }

    function test_customizer_data() { return TextAudit.cases() }
    function test_customizer(data) {
        found = []
        I18n.choose(data.language)
        // The modal sizes itself on the window's overlay: a typical window and the smallest.
        for (const room of [1400, 960]) {
            for (const grouped of [true, false]) {
                test.width = room
                const customizer = createTemporaryObject(customizerFactory, test, {
                    grouped: grouped, sections: test.shelves, groupSources: [{ id: "fixture", name: "Fixture", icon: "" }] })
                customizer.open()
                tryVerify(() => customizer.opened, 2000)
                audit(customizer.contentItem, data, (grouped ? "Home's" : "a source's") + " customizer in a window " + room + " wide")
                customizer.close()
                customizer.destroy()
                wait(0)
            }
        }
        compare(found.length, 0, found.join("\n"))
    }
}
