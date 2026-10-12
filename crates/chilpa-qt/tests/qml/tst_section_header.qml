import QtQuick
import QtTest
import "../../qml/Components"

// A shelf that scrolls sideways pages with two chevrons after its link, as
// QBZ's rails; each is enabled while there is more that way.
TestCase {
    name: "SectionHeader"
    when: windowShown
    visible: true
    width: 800
    height: 200

    SectionHeader { id: head; width: 700; title: "Press accolades"; link: "See all" }
    SignalSpy { id: forward; target: head; signalName: "forward" }
    SignalSpy { id: back; target: head; signalName: "back" }

    function test_the_chevrons_page_only_where_there_is_more() {
        const left = findChild(head, "shelfBack"), right = findChild(head, "shelfForward")
        verify(!left.visible && !right.visible, "a shelf that does not scroll has none")
        head.paged = true
        head.canBack = false
        head.canForward = true
        verify(left.visible && right.visible)
        verify(!left.enabled, "nothing back at the start")
        verify(right.enabled)
        waitForRendering(right)
        mouseClick(right, right.width / 2, right.height / 2)
        compare(forward.count, 1)
        mouseClick(left, left.width / 2, left.height / 2)
        compare(back.count, 0, "a disabled chevron does nothing")
    }
}
