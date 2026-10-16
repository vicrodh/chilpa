import QtQuick
import QtTest
import app.chilpa.graphics
import "../../qml/Components"

// The probe says what draws once it is known, and the watchdog reports the
// frames of its window once, then lets the window go.
TestCase {
    id: test
    name: "GraphicsWatch"
    when: windowShown
    visible: true
    width: 200
    height: 100

    Rectangle { id: moving; width: 10; height: 10; color: "black" }
    GraphicsWatch { id: watch; window: test.Window.window }

    function test_the_api_is_said_by_name() {
        verify(Graphics.apiReports >= 1, "the probe reports at start")
        verify(["software", "opengl", "vulkan", "metal", "d3d11", "d3d12", "null", "unknown"].indexOf(Graphics.api) >= 0, Graphics.api)
    }

    function test_the_liveness_is_reported_once_with_the_frames() {
        for (let i = 0; i < 5; ++i) {
            moving.x = i * 10
            wait(20)
        }
        tryCompare(Graphics, "livenessReports", 1, 2000, "one report after the liveness window")
        compare(Graphics.liveness, watch.frames)
        verify(watch.reported)
        const after = watch.frames
        moving.x = 100
        wait(100)
        compare(watch.frames, after, "after the report the window is let go")
        compare(Graphics.livenessReports, 1, "and it never reports again")
        if (after > 0)
            compare(Graphics.shown, 1, "the first frame told the crash chain, once")
    }
}
