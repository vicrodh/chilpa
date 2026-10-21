import QtQuick
import QtTest
import app.chilpa.kit
import app.chilpa.immersive
import "../../qml/Patterns"

TestCase {
    name: "ImmersiveFavoriteFeedback"
    when: windowShown
    visible: true
    width: 900; height: 200
    ImmersivePlayerBar { id: bar; shaders: false }
    function init() { Immersive.favoritePending = true }
    function cleanup() { Immersive.favoritePending = false; bar.opacity = 1 }
    function test_the_same_pending_bar_stops_when_faded_and_resumes_when_shown() {
        compare(Pulse.held, 1)
        bar.opacity = 0
        compare(Pulse.held, 0)
        bar.opacity = 1
        compare(Pulse.held, 1)
    }
}
