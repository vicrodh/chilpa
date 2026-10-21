import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/mini/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/immersive/qml/Components"

TestCase {
    id: test
    name: "LyricsClocks"
    when: windowShown
    Component {
        id: factory
        Window {
            width: 500; height: 600; visible: true
            property alias feed: feed
            LyricsPanel { anchors.fill: parent }
            MiniLyricsSurface { anchors.fill: parent }
            LyricsFeed { id: feed }
        }
    }
    function test_clocks_release_on_pause_hide_and_destruction() {
        Pulse.automatic = false
        Player.playing = true
        Player.paused = false
        Player.lyricsJson = JSON.stringify({source: "fixture", document: {timing: "word", lines: [{text: "Line", start_ms: 0}]}})
        const window = factory.createObject(test)
        tryCompare(Pulse, "held", 3)
        Player.paused = true
        compare(Pulse.held, 0)
        Player.paused = false
        compare(Pulse.held, 3)
        window.hide()
        tryCompare(Pulse, "held", 0)
        window.show()
        tryCompare(Pulse, "held", 3)
        window.feed.live = false
        compare(Pulse.held, 2)
        window.destroy()
        tryCompare(Pulse, "held", 0)
        Player.lyricsJson = ""
        Pulse.automatic = true
    }
}
