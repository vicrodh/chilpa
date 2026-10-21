import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/mini/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/immersive/qml/Components"
import "../../qml/Components"

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
    Component {
        id: linesFactory
        Window {
            width: 500; height: 600; visible: true
            property alias verses: verses
            LyricLines {
                id: verses
                anchors.fill: parent
                synced: true
                current: 0
                lines: [{text: "First words", start_ms: 0}, {text: "Next words", start_ms: 1000}, {text: "Last words", start_ms: 2000}]
            }
        }
    }
    SignalSpy { id: frames; signalName: "frameSwapped" }
    function test_hidden_line_changes_present_nothing() {
        const window = linesFactory.createObject(test)
        frames.target = window
        wait(500)
        window.verses.visible = false
        wait(500)
        frames.clear()
        for (const index of [1, 2, 0]) {
            window.verses.current = index
            wait(250)
        }
        compare(frames.count, 0)
        frames.target = null
        window.destroy()
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
