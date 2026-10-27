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
    ListenListScreen { id: page; anchors.fill: parent; visualizerFeed: feed; visualizationsReady: true }
    QtObject {
        id: feed
        property var bars: []
        property var energy: []
        property var waveform: []
        property var goniometer: []
        property var oscilloscope: []
        property bool capturing: false
        function setCompact(shown, mode) { capturing = shown }
    }


    function cleanup() { page.visible = true; panel().visualization = false; SettingsModel.keep("ui.queue.visualizer", "false"); AudioOutput.pathAnswer = "{}"; panel().show("playing"); NowAbout.trackAnswer = ({}); NowAbout.albumAnswer = ({}) }
    function panel() { return findChild(page, "listenNow") }
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

    function test_visualization_toggle_cycles_and_releases_the_consumer() {
        const toggle = findChild(page, "queueVisualizationToggle")
        const animation = findChild(page, "queueVisualization")
        compare(animation.active, false)
        verify(toggle.enabled)
        const idleGlyph = toggle.glyph
        mouseClick(toggle)
        tryVerify(() => animation.item !== null)
        compare(SettingsModel.kept("ui.queue.visualizer"), "true")
        verify(toggle.glyph !== idleGlyph)
        animation.item.cycleRequested(5)
        compare(panel().visualizationMode, 5)
        compare(SettingsModel.kept("ui.queue.visualizer.mode"), "5")
        panel().show("about")
        tryCompare(animation, "item", null)
        compare(feed.capturing, false)
        panel().show("playing")
        tryVerify(() => animation.item !== null)
        compare(animation.item.mode, 5)
        mouseClick(toggle)
        tryCompare(animation, "item", null)
        compare(feed.capturing, false)
    }

    function test_signal_path_moves_up_without_animation_and_reads_only_while_shown() {
        AudioOutput.pathAnswer = JSON.stringify({status:"unverified",summary:"Path unverified",stages:[
            {label:"Source",value:"FLAC 24/96"},{label:"Device",value:"Fixture DAC",mode:"Mode not reported"}]})
        panel().refreshPath()
        const host = findChild(page, "queueSignalPathHost")
        tryVerify(() => host.item !== null)
        compare(findChild(host.item, "queueSignalPathStatus").text, "Path unverified")
        const without = host.y
        panel().toggleVisualization()
        tryVerify(() => host.y > without)
        panel().toggleVisualization()
        tryCompare(host, "y", without)
        const reads = AudioOutput.pathReads
        page.visible = false
        page.tick()
        compare(AudioOutput.pathReads, reads)
        compare(host.item, null)
        page.visible = true
        tryVerify(() => host.item !== null)
        verify(AudioOutput.pathReads > reads)
    }

    // The lyrics fill the column with no header, keep a size of their own,
    // Small at first, and have their settings at the end of the tabs' row.
    function test_the_lyrics_tab_is_only_the_lines_with_its_settings_by_the_tabs() {
        const button = findChild(page, "queueLyricsOptions")
        verify(!button.visible, "only beside the lyrics")
        findChild(page, "queuePanelTabs").activated(1)
        let lyrics = null
        tryVerify(() => { lyrics = findChild(page, "queuePanelLyrics").item; return lyrics !== null && button.visible })
        compare(findChild(lyrics, "lyricsList").width, lyrics.width, "edge to edge")
        verify(!findChild(lyrics, "lyricsOptionsButton").visible, "no header saying the track a third time")
        const options = findChild(lyrics, "lyricsOptions")
        compare(options.size, "s")
        options.choose("size", "l")
        compare([SettingsModel.kept("ui.lyrics.size.queue"), SettingsModel.kept("ui.lyrics.size") || ""], ["l", ""], "the sidebar's size is not touched")
        mouseClick(button)
        tryVerify(() => lyrics.optionsOpen && button.checked)
        lyrics.toggleOptions()
        tryVerify(() => !lyrics.optionsOpen)
        options.choose("size", "s")
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
