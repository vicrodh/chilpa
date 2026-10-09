import QtQuick
import QtTest
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The playing row's mark: still bars unless Appearance asks for the moving
// ones, which exist only while the track runs and hold the pulse only then.
TestCase {
    id: test
    name: "PlayingMark"
    when: windowShown
    visible: true
    width: 200
    height: 100

    PlayingIndicator { id: mark; objectName: "mark" }
    ListRowSurface { id: surface; y: 40; width: 200; height: 40 }


    function cleanup() {
        Prefs.animatedIndicator = false
        mark.running = false
        mark.visible = true
        surface.edge = false
    }

    function test_still_by_default() {
        mark.running = true
        compare(mark.moving, false)
        compare(mark.children[0].item, null, "no moving bars while the option is off")
        compare(Pulse.held, 0)
    }

    function test_moves_only_while_running() {
        Prefs.animatedIndicator = true
        compare(mark.children[0].item, null, "a paused track builds no bars")
        compare(Pulse.held, 0)
        mark.running = true
        verify(mark.children[0].item !== null, "the running track gets the moving bars")
        compare(Pulse.held, 1, "the bars hold the pulse")
        const before = mark.children[0].item.t
        tryVerify(() => mark.children[0].item.t !== before, 500, "the bars move on the pulse")
        mark.running = false
        compare(mark.children[0].item, null)
        compare(Pulse.held, 0, "pausing lets the pulse go")
    }

    function test_hidden_mark_builds_nothing() {
        Prefs.animatedIndicator = true
        mark.running = true
        mark.visible = false
        compare(mark.children[0].item, null)
        compare(Pulse.held, 0)
    }

    function test_edge_mark() {
        const edge = surface.children[0]
        compare(edge.visible, false)
        surface.edge = true
        compare(edge.visible, true)
        compare(edge.width, Metrics.playingEdge)
        compare(edge.height, surface.height - 2 * Metrics.playingEdgeInset)
    }
}
