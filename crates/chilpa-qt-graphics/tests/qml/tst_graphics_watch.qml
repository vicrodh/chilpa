import QtQuick
import QtTest
import app.chilpa.graphics
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
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

    function test_without_the_gpu_tier_motion_is_reduced() {
        Graphics.gpuTier = true
        compare(Motion.reduced, false, "a GPU keeps every animation")
        compare(Motion.page, 300)
        Graphics.gpuTier = false
        compare(Motion.reduced, true, "a rasterizer without shaders gets the reduced cadence")
        compare(Motion.page, 0)
        Graphics.gpuTier = true
        compare(Motion.reduced, false)
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
