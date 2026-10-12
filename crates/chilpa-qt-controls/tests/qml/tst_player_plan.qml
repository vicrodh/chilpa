import QtQuick
import QtTest
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Patterns"

// The three layouts of the player: floating over the content, or the
// window's width at its bottom or its top with the planes stopping at it.
TestCase {
    name: "PlayerPlan"
    when: windowShown
    visible: true
    width: 1200
    height: 800

    Item { id: frame; width: 1200; height: 800 }
    Item { id: host; x: 260; y: plan.planeTop; width: 900; height: frame.height - y - plan.contentBottom }
    PlayerPlan { id: plan; window: frame; content: host; tall: 80 }

    function cleanup() { PlayerLayout.mode = "floating"; plan.away = false }

    function test_floating_spans_the_content_over_its_end() {
        compare(plan.planeTop, Metrics.tabBarHeight)
        compare(plan.planeBottom, Metrics.spaceWindow, "the side planes keep the window's margin")
        compare(plan.barX, host.x + Metrics.spaceWindow)
        compare(plan.barWidth, host.width - Metrics.spaceWindow * 2)
        compare(plan.barY + 80, host.y + host.height - Metrics.spaceWindow, "over the content's end")
        plan.away = true
        compare(plan.barY, frame.height, "a still pointer sends it away")
    }

    function test_at_the_bottom_it_spans_the_window_and_the_planes_stop_above_it() {
        PlayerLayout.mode = "bottom"
        compare(plan.barX, Metrics.spaceWindow)
        compare(plan.barWidth, frame.width - Metrics.spaceWindow * 2)
        compare(plan.barY, frame.height - 80 - Metrics.spaceWindow)
        compare(host.y + host.height, plan.barY - Metrics.spaceWindow, "the content ends a gap above it")
        compare(frame.height - plan.planeBottom, plan.barY - Metrics.spaceWindow, "and so do the side planes")
        plan.away = true
        compare(plan.barY, frame.height - 80 - Metrics.spaceWindow, "docked, it never steps away")
        compare(Metrics.shellBottom, plan.planeBottom, "the planes read where they end")
    }

    function test_at_the_top_it_sits_under_the_tab_bar_and_the_planes_start_under_it() {
        PlayerLayout.mode = "top"
        compare(plan.barY, Metrics.tabBarHeight)
        compare(plan.planeTop, Metrics.tabBarHeight + 80 + Metrics.spaceWindow)
        compare(host.y, plan.barY + 80 + Metrics.spaceWindow)
        compare(Metrics.shellTop, plan.planeTop)
        plan.tall = 105
        compare(host.y, Metrics.tabBarHeight + 105 + Metrics.spaceWindow, "the band opening moves the planes down")
        plan.tall = 80
    }
}
