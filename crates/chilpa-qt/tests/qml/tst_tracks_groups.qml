import QtQuick
import QtTest
import app.chilpa
import "../../qml/Screens"

TestCase {
    name: "TracksGroups"
    when: windowShown
    visible: true
    width: 1200
    height: 800

    TracksScreen { id: page; width: 1200; height: 800 }
    Component { id: albumsPage; AlbumsScreen {} }
    Component { id: libraryPage; LibraryScreen {} }

    function test_record_menus_offer_and_keep_plays_data() {
        return [{tag: "Albums", factory: albumsPage, key: "albums"},
                {tag: "Library", factory: libraryPage, key: "library"}]
    }
    function test_record_menus_offer_and_keep_plays(data) {
        const screen = createTemporaryObject(data.factory, this, {width: 1200, height: 800})
        verify(screen)
        const grouping = findChild(screen, data.key + "Grouping")
        const at = grouping.options.findIndex(option => option.value === "plays")
        verify(at >= 0)
        findChild(grouping, "sortMenu").chosen(at)
        compare(screen.group, "plays")
        compare(SettingsModel.kept("ui." + data.key + ".group"), "plays")
        grouping.picked("")
    }

    function fill(label) {
        const model = page.scrollView.model
        model.clear()
        for (let i = 0; i < 80; ++i)
            model.append({ trackId: i + 1, rowKind: 0, number: i + 1, title: "Track " + i,
                artist: "Artist", album: "Record", sourceName: "", sourceInfo: "{}", composer: "",
                duration: "3:00", plays: 0, quality: "FLAC", tier: 2, available: true,
                playing: false, selected: false, headerLabel: "", headerMeta: "", art: "", group: label })
        page.scrollView.forceLayout()
        page.scrollView.positionViewAtBeginning()
        wait(0)
    }

    function test_menu_keeps_each_group_and_displays_the_current_header() {
        const grouping = findChild(page, "tracksGrouping")
        compare(grouping.options.map(option => option.value),
                ["off", "album", "artist", "name", "year", "decade", "genre", "label", "composer", "plays"])
        const labels = ["1999", "1990s", "Jazz", "House", "Writer", "Never played"]
        for (let i = 4; i < grouping.options.length; ++i) {
            const group = grouping.options[i].value
            findChild(grouping, "sortMenu").chosen(i)
            compare(page.group, group)
            compare(SettingsModel.kept("ui.tracks.group"), group)
            compare(page.scrollView.model.key.split("\u001f")[1], group)
            fill(labels[i - 4])
            tryCompare(findChild(page, "tracksGroupName"), "text", labels[i - 4])
            page.letters = i < 6 || group === "plays" ? [] : [[labels[i - 4][0], 0]]
            compare(findChild(page, "tracksLetters").visible, i >= 6 && group !== "plays")
            compare(page.scrollView.section.property, "group")
        }
        grouping.picked("off")
        compare(findChild(page, "tracksGroupName").visible, false)
        compare(findChild(page, "tracksLetters").visible, false)
        compare(page.scrollView.section.property, "")
    }

    function test_tracks_sort_by_record_both_ways() {
        const order = findChild(page, "tracksGrouping").parent.children.find(c => c.options && c.options.some(o => o.value === "duration"))
        const at = order.options.findIndex(option => option.value === "album")
        findChild(order, "sortMenu").chosen(at); compare(page.order, "album")
        findChild(order, "sortMenu").chosen(at); compare(page.order, "album-desc", "a second pick turns it around")
        order.picked("default")
    }
    function test_header_follows_the_visible_group_after_scrolling() {
        findChild(page, "tracksGrouping").picked("genre")
        fill("Jazz")
        for (let i = 20; i < 80; ++i)
            page.scrollView.model.setProperty(i, "group", "Rock")
        page.scrollView.forceLayout()
        tryCompare(findChild(page, "tracksGroupName"), "text", "Jazz")
        page.scrollView.positionViewAtIndex(25, ListView.Beginning)
        tryCompare(findChild(page, "tracksGroupName"), "text", "Rock")
        page.scrollView.positionViewAtBeginning()
        tryCompare(findChild(page, "tracksGroupName"), "text", "Jazz")
    }
}
