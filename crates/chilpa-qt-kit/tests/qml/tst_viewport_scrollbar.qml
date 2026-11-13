import QtQuick
import QtQuick.Controls.Basic
import QtTest
import "../../qml/Primitives"
import "../../qml/Theme"

TestCase {
    id: test
    name: "ViewportScrollbar"
    when: windowShown
    visible: true
    width: 800
    height: 400
    Flickable {
        id: view
        x: 20; y: 20; width: 400; height: 300
        contentWidth: width
        contentHeight: 1000
        clip: true
        ScrollBar.vertical: ViewportScrollBar { id: bar }
    }
    Flickable {
        id: longView
        x: 430; y: 20; width: 50; height: 300
        contentWidth: width; contentHeight: 19000 * 36
        clip: true
        ScrollBar.vertical: ViewportScrollBar { id: longBar }
        MouseArea { id: rowPointer; anchors.fill: parent; hoverEnabled: true }
    }
    Flickable {
        id: wideView
        x: 20; y: 330; width: 400; height: 32
        contentWidth: 1000; contentHeight: height
        clip: true
        ScrollBar.horizontal: ViewportScrollBar { id: wideBar }
    }
    Flickable {
        id: overlayView
        x: 20; y: 365; width: 400; height: 30
        contentWidth: width; contentHeight: 1000
        clip: true
        ScrollBar.vertical: ViewportScrollBar { id: overlayBar; overlayParent: test; z: 2 }
    }
    Rectangle { x: 300; y: 365; width: 120; height: 30; z: 1 }
    Flickable {
        id: gutterView
        x: 550; y: 20; width: 180; height: 300
        contentWidth: width; contentHeight: 1000
        clip: true
        ScrollBar.vertical: ViewportScrollBar { id: gutterBar; gutter: Metrics.pageGutter }
    }
    Component {
        id: clearedViewMaker
        Item {
            id: frame
            x: 20; y: 20; width: 480; height: 300; z: 10
            property real gutter: -Metrics.gap8
            property alias view: clearedView
            property alias vertical: vertical
            property alias horizontal: horizontal
            property alias player: cover
            property int blockedPresses: 0
            Flickable {
                id: clearedView
                width: 400; height: parent.height
                contentWidth: 1000; contentHeight: 2000
                clip: true
                ScrollBar.vertical: ViewportScrollBar {
                    id: vertical
                    policy: ScrollBar.AlwaysOn
                    gutter: frame.gutter
                    bottomClearance: Metrics.floatingPlayerClearance
                    z: 2
                }
                ScrollBar.horizontal: ViewportScrollBar {
                    id: horizontal
                    policy: ScrollBar.AlwaysOn
                    gutter: frame.gutter
                    bottomClearance: Metrics.floatingPlayerClearance
                    z: 2
                }
            }
            Rectangle {
                id: cover
                x: Metrics.spaceWindow
                y: parent.height - height - Metrics.spaceWindow
                width: clearedView.width - Metrics.spaceWindow * 2
                height: Metrics.playerSpace
                visible: height > 0
                color: "gray"
                z: 5
                MouseArea { anchors.fill: parent; onPressed: frame.blockedPresses++ }
            }
        }
    }
    function init() { failOnWarning(/ReferenceError|TypeError|Binding loop detected/) }
    function cleanup() { Metrics.playerSpace = Metrics.playerHeight; mouseMove(test, 790, 390) }
    function assertVisibleThumb(scrollbar) {
        tryVerify(() => scrollbar.contentItem.width > 0 && scrollbar.contentItem.height > 0)
        verify(scrollbar.contentItem.opacity > 0)
    }
    function test_both_axes_have_nonzero_visible_thumbs() {
        const oldHorizontal = wideBar.policy, oldVertical = gutterBar.policy
        try {
            wideBar.policy = ScrollBar.AlwaysOn
            gutterBar.policy = ScrollBar.AlwaysOn
            tryCompare(wideBar, "visible", true)
            tryCompare(gutterBar, "visible", true)
            assertVisibleThumb(wideBar)
            assertVisibleThumb(gutterBar)
            verify(gutterBar.x >= gutterView.x + gutterView.width)
        } finally {
            wideBar.policy = oldHorizontal
            gutterBar.policy = oldVertical
        }
    }
    function test_hover_crosses_the_free_gutter_to_the_thumb() {
        tryCompare(gutterBar, "parent", this)
        mouseMove(gutterView, 150, 120)
        tryCompare(gutterBar, "visible", true)
        mouseMove(this, gutterBar.x - Metrics.gap4, gutterBar.y + 120)
        compare(gutterBar.visible, true)
        mouseMove(this, gutterBar.x + gutterBar.width / 2, gutterBar.y + 120)
        compare(gutterBar.visible, true)
    }
    function test_overlay_panel_keeps_scrollbar_at_page_edge() {
        tryCompare(overlayBar, "parent", this)
        compare(overlayBar.x, overlayView.x + overlayView.width - Metrics.gap8 - overlayBar.width / 2)
        compare(overlayBar.y, overlayView.y)
        verify(overlayBar.z > 1)
    }
    function test_horizontal_thumb_uses_the_reserved_bottom_margin() {
        tryVerify(() => wideBar.parent && wideBar.parent.parent === wideView)
        compare(wideBar.width, wideView.width)
        compare(wideBar.y, wideView.height - Metrics.gap4 - wideBar.height / 2)
        verify(wideBar.y >= 0 && wideBar.y + wideBar.height <= wideView.height)
        mouseMove(wideView, 20, 20)
        tryCompare(wideBar, "visible", true)
    }
    function test_long_library_has_a_visible_grabbable_thumb() {
        tryVerify(() => longBar.parent && longBar.parent.parent === longView)
        compare(longBar.width, Metrics.scrollbarWidth)
        compare(longBar.x, longView.width - Metrics.gap8 - longBar.width / 2)
        verify(longBar.x >= 0 && longBar.x + longBar.width <= longView.width)
        verify(!longBar.visible)
        verify(longBar.minimumSize * longBar.height >= Metrics.scrollbarThumbMin)
        mouseMove(longView, 20, 30)
        tryCompare(longBar, "visible", true)
        verify(longBar.contentItem.opacity > 0)
    }
    function test_full_surface_pointer_can_reveal_thumb() {
        longBar.viewportHovered = Qt.binding(function() { return rowPointer.containsMouse })
        mouseMove(longView, 12, 120)
        tryCompare(rowPointer, "containsMouse", true)
        tryCompare(longBar, "visible", true)
    }
    function test_hover_reveals_thumb_in_margin_without_scrolling() {
        tryVerify(() => bar.parent && bar.parent.parent === view)
        wait(20)
        compare(bar.viewport, view)
        compare(bar.x, view.width - Metrics.gap8 - bar.width / 2)
        verify(bar.x >= 0 && bar.x + bar.width <= view.width)
        verify(!bar.visible)
        mouseMove(view, 100, 100)
        tryCompare(bar, "active", true)
        tryCompare(bar, "visible", true)
        compare(view.contentY, 0)
        mouseMove(this, 490, 390)
        tryCompare(bar, "active", false)
        tryCompare(bar, "visible", false)
    }
    function test_player_clearance_preserves_both_axes_and_hover_bridge_data() {
        return [
            {tag:"normal-inside", space:Metrics.playerHeight, gutter:-Metrics.gap8},
            {tag:"normal-gutter", space:Metrics.playerHeight, gutter:Metrics.pageGutter},
            {tag:"waveform-inside", space:Metrics.playerHeightTall, gutter:-Metrics.gap8},
            {tag:"waveform-gutter", space:Metrics.playerHeightTall, gutter:Metrics.pageGutter},
            {tag:"docked-inside", space:0, gutter:-Metrics.gap8},
            {tag:"docked-gutter", space:0, gutter:Metrics.pageGutter}
        ]
    }
    function test_player_clearance_preserves_both_axes_and_hover_bridge(data) {
        Metrics.playerSpace = data.space
        const frame = createTemporaryObject(clearedViewMaker, test, {gutter:data.gutter})
        verify(waitForRendering(frame))
        const clearance = data.space > 0 ? data.space + Metrics.spaceWindow : 0
        compare(frame.vertical.reservedBottom, clearance)
        compare(frame.horizontal.reservedBottom, clearance)
        tryCompare(frame.vertical, "height", frame.view.height - clearance)
        compare(frame.horizontal.width, frame.view.width)
        if (clearance > 0) {
            verify(frame.vertical.mapToItem(frame, 0, frame.vertical.height).y <= frame.player.y + 0.5)
            tryVerify(() => frame.horizontal.mapToItem(frame, 0, frame.horizontal.height).y <= frame.player.y + 0.5)
        } else {
            compare(frame.vertical.height, frame.view.height)
            compare(frame.horizontal.mapToItem(frame, 0, 0).y, frame.view.height + data.gutter - frame.horizontal.height / 2)
        }
        if (data.gutter >= 0) {
            compare(frame.vertical.area.height, frame.vertical.height)
            if (clearance > 0) {
                compare(frame.horizontal.area.y, frame.horizontal.y)
                compare(frame.horizontal.area.height, frame.horizontal.height)
            }
        }
    }
    function test_dragging_both_thumbs_above_player_reaches_the_end_data() {
        return [{tag:"inside", gutter:-Metrics.gap8}, {tag:"outside", gutter:Metrics.pageGutter}]
    }
    function test_dragging_both_thumbs_above_player_reaches_the_end(data) {
        Metrics.playerSpace = Metrics.playerHeightTall
        const frame = createTemporaryObject(clearedViewMaker, test, {gutter:data.gutter})
        verify(waitForRendering(frame))
        const vertical = frame.vertical, horizontal = frame.horizontal
        const startY = vertical.contentItem.y + vertical.contentItem.height / 2
        mouseDrag(vertical, vertical.width / 2, startY, 0, vertical.height - startY)
        tryVerify(() => frame.view.atYEnd, 1000, "the shortened vertical track still reaches the last row")
        mouseClick(horizontal, horizontal.width - 2, horizontal.height / 2)
        verify(frame.view.contentX > 0, "the horizontal track is clickable above the player")
        frame.view.contentX = 0
        const startX = horizontal.contentItem.x + horizontal.contentItem.width / 2
        mouseDrag(horizontal, startX, horizontal.height / 2, horizontal.width - startX, 0)
        tryVerify(() => frame.view.atXEnd, 1000, "horizontal dragging still reaches the last column")
        compare(frame.blockedPresses, 0)
    }
    function test_destroying_viewport_before_deferred_positioning_is_safe() {
        const before = test.children.length
        const frame = createTemporaryObject(clearedViewMaker, test)
        frame.destroy()
        tryVerify(() => test.children.length === before)
        wait(1)
    }
}
