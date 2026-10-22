import QtQuick
import QtTest
import app.chilpa
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/controls/qml/Screens"

// Under the queue's art, three tabs: progress and quality, the lyrics as the
// player bar opens them, and what is known of the track and its record. The
// tools above the rows end where the rows do.
TestCase {
    name: "QueuePanel"
    when: windowShown
    visible: true
    width: 1500; height: 900
    ListenListScreen { id: page; anchors.fill: parent }

    function cleanup() { panel().show("playing"); NowAbout.trackAnswer = ({}); NowAbout.albumAnswer = ({}) }
    function panel() { return findChild(page, "queuePanelTabs").parent.parent }
    function shown(item, name) {
        let found = []
        const walk = node => { if (node.objectName === name && node.visible) found.push(node); for (const child of node.children || []) walk(child) }
        walk(item)
        return found
    }

    function test_the_tools_end_where_the_rows_do() {
        const search = findChild(page, "queueSearch"), list = findChild(page, "queueList")
        verify(search.visible && list.visible)
        const right = item => item.mapToItem(page, item.width, 0).x
        tryVerify(() => Math.abs(right(search) - right(list)) < 1, 1000, "search " + right(search) + " rows " + right(list))
    }

    function test_three_tabs_and_the_one_in_view_is_kept() {
        const tabs = findChild(page, "queuePanelTabs")
        verify(tabs.visible, "the panel shows on a wide page")
        compare(tabs.current, 0)
        verify(findChild(page, "queuePanelPlaying").visible, "progress and quality first")
        verify(!findChild(page, "queuePanelLyrics").active, "the lyrics are not built while hidden")
        tabs.activated(1)
        tryVerify(() => findChild(page, "queuePanelLyrics").item !== null)
        verify(!findChild(page, "queuePanelPlaying").visible)
        verify(findChild(page, "queuePanelLyrics").item.closable === false, "a tab has nothing to close")
        tabs.activated(2)
        compare(SettingsModel.kept("ui.queue.panel"), "about")
        tryVerify(() => findChild(page, "queuePanelAbout").item !== null)
        verify(!findChild(page, "queuePanelLyrics").active)
    }

    // The track by default, the record on request; who said it, and where.
    function test_about_tells_the_track_and_its_record() {
        NowAbout.trackAnswer = {loading: false, facts: [{label: "Composer", value: "Bill Berry"}], credits: [{name: "Peter Buck", roles: "guitar"}],
            origin: "MusicBrainz", link: "https://musicbrainz.org/recording/x"}
        NowAbout.albumAnswer = {loading: false, facts: [], credits: [], description: "A review", origin: "Fixture"}
        findChild(page, "queuePanelTabs").activated(2)
        let about = null
        tryVerify(() => { about = findChild(page, "queuePanelAbout").item; return about !== null })
        compare(about.part, "track")
        compare(about.facts.length, 1)
        compare(about.credits[0].name, "Peter Buck")
        verify(findChild(about, "queueAboutLink").visible, "MusicBrainz's page can be opened")
        findChild(about, "queueAboutParts").activated(1)
        compare(about.part, "album")
        verify(findChild(about, "queueAboutDescription").visible)
        compare(findChild(about, "queueAboutDescription").text, "A review")
        verify(!findChild(about, "queueAboutLink").visible)
        NowAbout.albumAnswer = {loading: false}
        findChild(about, "queueAboutParts").activated(0)
        findChild(about, "queueAboutParts").activated(1)
        tryVerify(() => findChild(about, "queueAboutNothing").visible, 1000, "nothing known says so")
    }
}
