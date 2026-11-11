import QtQuick
import QtTest
import app.chilpa
import app.chilpa.controls
import "../../../chilpa-qt-kit/qml/Theme"
import "../../qml/Components"
import "../../qml/Patterns"

TestCase {
    id: test
    name: "TrackInfoRequests"
    when: windowShown
    visible: true
    width: 900
    height: 700

    TrackInfoRequests { id: requests }
    TrackInfoRequests { id: galleryCopy; listenForRequests: false }
    SearchModel {
        id: search
        property var menuEntries: []
        function menuJson(index) { return JSON.stringify(menuEntries) }
    }
    SearchMenu { id: searchMenu; search: search }
    Component { id: playingInfo; TrackInfoModal {} }
    SignalSpy { id: asked; target: NowAbout; signalName: "trackInfoRequested" }
    SignalSpy { id: left; target: searchMenu; signalName: "left" }

    function host(of) { return findChild(of, "trackInfoRequestHost") }
    function init() {
        Player.path = "/music/Playing.flac"
        Player.title = "Playing"
        NowAbout.trackAnswer = ({loading:false, facts:[], credits:[], file:{title:"File", value:"/music/Playing.flac", note:""}})
        NowAbout.trackAnswers = ({"/music/Gone/Chosen.flac": {loading:false, facts:[], credits:[], file:{title:"File", value:"/music/Gone/Chosen.flac", note:"Not found on disk"}}})
        NowAbout.albumAnswer = ({loading:false, facts:[], credits:[]})
        NowAbout.retried = ({})
        search.requestedAction = ""
        asked.clear()
        left.clear()
    }
    function cleanup() {
        if (host(requests).item) host(requests).item.close()
        tryCompare(host(requests), "active", false)
        searchMenu.close()
    }

    function test_a_chosen_track_shows_its_own_information_not_the_playing_one() {
        compare(host(requests).active, false, "no modal exists until one is asked for")
        NowAbout.trackInfoRequested("/music/Gone/Chosen.flac", "Chosen")
        tryCompare(host(requests), "active", true)
        const modal = host(requests).item
        tryCompare(modal, "opened", true)
        compare(findChild(modal, "trackInfoTitle").text, "Chosen")
        tryCompare(findChild(modal, "queueAboutFilePath"), "text", "/music/Gone/Chosen.flac")
        compare(findChild(modal, "queueAboutFileNote").text, "Not found on disk")
        compare(NowAbout.askedPath, "/music/Gone/Chosen.flac")
        const about = findChild(modal, "trackInfoAbout")
        about.part = "album"
        tryCompare(NowAbout, "askedPath", "/music/Gone/Chosen.flac")
        compare(Player.path, "/music/Playing.flac", "what plays stays")
        compare(host(galleryCopy).active, false, "the Gallery's copy does not answer")
        keyClick(Qt.Key_Escape)
        tryCompare(host(requests), "active", false)
    }

    function test_a_retry_from_a_chosen_track_retries_that_track() {
        NowAbout.trackAnswers = ({"/music/Chosen.flac": {loading:false, failed:true}})
        NowAbout.trackInfoRequested("/music/Chosen.flac", "Chosen")
        tryCompare(host(requests), "active", true)
        const retry = findChild(host(requests).item, "queueAboutRetry")
        tryCompare(retry, "visible", true)
        retry.clicked()
        compare(NowAbout.retried.path, "/music/Chosen.flac")
    }

    function test_the_player_title_still_opens_what_plays() {
        const modal = createTemporaryObject(playingInfo, test)
        modal.open()
        tryCompare(modal, "opened", true)
        compare(findChild(modal, "trackInfoTitle").text, "Playing")
        tryCompare(findChild(modal, "queueAboutFilePath"), "text", "/music/Playing.flac")
        modal.close()
    }

    function test_the_search_menu_asks_for_a_found_song_without_acting_on_it() {
        search.menuEntries = [{act:"play", text:"Play now"}, {act:"info", text:"Track info", path:"/music/Found.flac", title:"Found"}]
        searchMenu.show(1, 0, 0)
        tryCompare(searchMenu, "opened", true)
        compare(searchMenu.entries[1].glyph, Icons.infoCircle)
        searchMenu.chosen(1)
        compare(asked.count, 1)
        compare([asked.signalArguments[0][0], asked.signalArguments[0][1]], ["/music/Found.flac", "Found"])
        compare(search.requestedAction, "", "the facade is not asked to act on it")
        compare(left.count, 0, "the results stay where they are")
    }
}
