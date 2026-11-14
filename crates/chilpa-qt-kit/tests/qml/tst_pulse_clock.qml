import QtQuick
import QtTest
import app.chilpa.kit
import "../../qml/Components"

TestCase {
    name:"PulseClockClaims"; when:windowShown
    Component { id:maker; PulseClock {} }
    SignalSpy { id:ticks; signalName:"triggered" }
    function test_slow_claims_switch_and_release_without_a_second_clock() {
        Pulse.automatic=false
        const held=Pulse.held,slow=Pulse.slowHeld
        const clock=createTemporaryObject(maker,this,{running:true,slow:true,minimumInterval:100})
        ticks.target=clock
        compare(Pulse.held,held+1);compare(Pulse.slowHeld,slow+1)
        Pulse.ms+=1;compare(ticks.count,1)
        Pulse.ms+=33;compare(ticks.count,1)
        Pulse.ms+=67;compare(ticks.count,2)
        clock.slow=false
        compare(Pulse.held,held+1);compare(Pulse.slowHeld,slow)
        clock.running=false;compare(Pulse.held,held)
        clock.destroy();wait(1);compare(Pulse.held,held)
        Pulse.automatic=true
    }
}
