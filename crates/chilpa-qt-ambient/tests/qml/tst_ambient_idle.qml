import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.ambient
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../../qml/Patterns"

TestCase {
    id: test
    name: "AmbientIdle"
    when: windowShown
    Component {
        id: factory
        Window {
            width: 400; height: 300; visible: true
            AppBackground { anchors.fill: parent }
            PlayingIndicator { x: 20; y: 20; running: Player.playing && !Player.paused }
        }
    }
    SignalSpy { id: frames; signalName: "frameSwapped" }
    function test_paused_and_hidden_surfaces_present_nothing() {
        Pulse.automatic = false
        Ambient.mode = 1
        Prefs.animatedIndicator = true
        Player.path = "/fixture.wav"
        Player.playing = true
        Player.paused = false
        const window = factory.createObject(test)
        frames.target = window
        tryVerify(() => Pulse.held > 0)
        Pulse.ms += 99
        wait(150)
        verify(frames.count > 0)
        Player.paused = true
        wait(150)
        compare(Pulse.held, 0)
        frames.clear()
        Pulse.ms += 99
        wait(200)
        compare(frames.count, 0)
        Player.paused = false
        window.hide()
        wait(150)
        compare(Pulse.held, 0)
        frames.clear()
        Pulse.ms += 99
        wait(200)
        compare(frames.count, 0)
        frames.target = null
        window.destroy()
        Pulse.automatic = true
    }
}
