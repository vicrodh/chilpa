import QtQuick
import QtTest
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

TestCase {
    id: test
    name: "PulseClock"
    when: windowShown
    Component { id: factory; PulseClock {} }
    SignalSpy { id: ticks; signalName: "triggered" }
    function init() { Pulse.automatic = false; ticks.clear() }
    function cleanup() { ticks.target = null; Pulse.automatic = true }
    function test_one_hold_only_while_running() {
        const clock = factory.createObject(test)
        ticks.target = clock
        compare(Pulse.held, 0)
        Pulse.ms += 33
        compare(ticks.count, 0)
        clock.running = true
        clock.sync()
        compare(Pulse.held, 1)
        Pulse.ms += 33
        compare(ticks.count, 1)
        clock.running = false
        compare(Pulse.held, 0)
        Pulse.ms += 33
        compare(ticks.count, 1)
        clock.running = true
        clock.destroy()
        tryCompare(Pulse, "held", 0)
    }
    function test_consumers_share_edges_and_release_independently() {
        const first = createTemporaryObject(factory, test, {running: true})
        const second = createTemporaryObject(factory, test, {running: true})
        ticks.target = second
        compare(Pulse.held, 2)
        first.running = false
        compare(Pulse.held, 1)
        Pulse.ms += 99
        compare(ticks.count, 1)
        second.running = false
        compare(Pulse.held, 0)
    }
}
