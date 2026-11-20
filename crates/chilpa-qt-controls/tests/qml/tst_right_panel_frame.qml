import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"

TestCase {
    name: "RightPanelFrame"
    when: windowShown
    visible: true
    width: 1000
    height: 640

    RightPanelFrame {
        id: frame
        x: parent.width - width
        height: parent.height
        open: SettingsModel.queueOpen || SettingsModel.lyricsOpen
    }

    Component {
        id: viewportFactory
        ContentPlane {
            moving: false
            leftInset: 0
            rightInset: 0
            topInset: 0
            bottomInset: 0
            Item { objectName: "heldContent"; anchors.fill: parent }
        }
    }

    function test_shared_content_gets_one_new_width_when_motion_ends() {
        const viewport = createTemporaryObject(viewportFactory, this)
        verify(viewport)
        const content = findChild(viewport, "heldContent")
        compare(content.width, width)
        viewport.moving = true
        for (const inset of [20, 40, 100, 185]) {
            viewport.leftInset = inset
            viewport.rightInset = inset
            compare(viewport.width, width - 2 * inset)
            compare(content.width, width)
        }
        viewport.moving = false
        compare(content.width, viewport.width)
    }

    function init() {
        SettingsModel.queueOpen = true
        tryCompare(frame, "width", frame.settled)
        tryCompare(frame, "moving", false)
    }

    function cleanup() {
        SettingsModel.queueOpen = false
        tryCompare(frame, "width", 0)
    }

    function test_close_keeps_the_section_painted_at_a_fixed_width() {
        const panel = findChild(frame, "rightPanelContent")
        SettingsModel.queueOpen = false
        tryVerify(() => frame.moving && frame.width > 0 && frame.width < frame.settled)
        verify(panel.queueShown)
        compare(panel.width, frame.settled)
        tryCompare(frame, "width", 0)
        compare(panel.visible, false)
    }

    function test_grip_follows_the_pointer_without_reflowing_the_panel() {
        const panel = findChild(frame, "rightPanelContent")
        const grip = findChild(frame, "rightPanelGrip")
        const width = frame.settled
        grip.started()
        grip.moved(40)
        compare(frame.width, SettingsModel.panelWidthAt(width - 40, frame.Window.width))
        compare(panel.width, width)
        grip.canceled()
        tryCompare(frame, "width", width)
    }
}
