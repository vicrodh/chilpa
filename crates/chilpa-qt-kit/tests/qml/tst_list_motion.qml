import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

TestCase {
    id: test
    name: "ListMotion"
    when: windowShown
    visible: true
    width: 500; height: 500
    property int created: 0
    property int destroyed: 0
    Component {
        id: fixture
        MotionListView {
            width: 400; height: 400
            model: TrackListModel {}
            delegate: Rectangle {
                required property string title
                width: 60; height: 60
                color: "steelblue"
                Component.onCompleted: test.created++
                Component.onDestruction: test.destroyed++
            }
        }
    }
    Component { id: viewportFixture; Flickable { width: 400; height: 100; contentHeight: 1000; clip: true } }
    Component { id: resetModel; TrackListModel { Component.onCompleted: append([{title:"a"}, {title:"b"}]) } }
    function init() { Motion.reduced = false; created = 0; destroyed = 0 }
    function cleanup() { Motion.reduced = false }
    function test_edits_keep_objects_data() {
        return [{tag:"vertical", horizontal:false}, {tag:"shelf", horizontal:true}]
    }
    function test_edits_keep_objects(data) {
        const view = createTemporaryObject(fixture, test, {orientation: data.horizontal ? ListView.Horizontal : ListView.Vertical})
        view.model.append([{title:"a"}, {title:"b"}, {title:"c"}])
        view.forceLayout()
        wait(Motion.panel + 50)
        const kept = view.itemAtIndex(1)
        verify(kept)
        const before = created
        view.model.insert(1, {title:"new"})
        view.forceLayout()
        compare(view.itemAtIndex(2), kept)
        verify(view.add.running || view.addDisplaced.running)
        tryCompare(view.add, "running", false)
        tryCompare(view.addDisplaced, "running", false)
        compare(created, before + 1)
        compare(view.itemAtIndex(1).opacity, 1)
        view.model.move(2, 0, 1)
        view.forceLayout()
        compare(view.itemAtIndex(0), kept)
        verify(view.move.running || view.moveDisplaced.running)
        tryCompare(view.move, "running", false)
        tryCompare(view.moveDisplaced, "running", false)
        const removed = destroyed
        view.model.remove(1)
        view.forceLayout()
        verify(view.remove.running || view.removeDisplaced.running)
        tryCompare(test, "destroyed", removed + 1)
        tryCompare(view.removeDisplaced, "running", false)
        compare(view.itemAtIndex(0), kept)
        compare(created, before + 1)
        compare(data.horizontal ? kept.x : kept.y, 0)
    }
    function test_reduced_and_hidden_finish_immediately() {
        const view = createTemporaryObject(fixture, test)
        view.model.append([{title:"a"}, {title:"b"}])
        view.forceLayout()
        wait(Motion.panel + 50)
        const kept = view.itemAtIndex(0)
        Motion.reduced = true
        compare(view.motionDuration, 0)
        compare(view.arrivalDuration, 0)
        view.model.insert(0, {title:"new"})
        view.forceLayout()
        verify(!view.add || !view.add.running)
        compare(view.itemAtIndex(1), kept)
        compare(kept.y, 60)
        Motion.reduced = false
        view.model.move(1, 0, 1)
        view.forceLayout()
        view.visible = false
        compare(view.motionDuration, 0)
        compare(view.move, null)
        view.model.remove(1)
        view.forceLayout()
        verify(!view.remove || !view.remove.running)
        view.visible = true
        compare(view.itemAtIndex(0), kept)
        compare(kept.y, 0)
    }
    function test_cancel_active_edits_data() {
        const cases = []
        for (const edit of ["insert", "move", "remove"])
            for (const reduced of [false, true]) cases.push({tag: edit + reduced, edit, reduced})
        return cases
    }
    function test_cancel_active_edits(data) {
        const view = createTemporaryObject(fixture, test)
        view.model = createTemporaryObject(resetModel, test)
        view.forceLayout()
        if (data.edit === "insert") view.model.insert(0, {title:"new"})
        else if (data.edit === "move") view.model.move(1, 0, 1)
        else view.model.remove(0)
        view.forceLayout()
        if (data.reduced) Motion.reduced = true
        else view.visible = false
        for (const transition of [view.add, view.addDisplaced, view.move, view.moveDisplaced, view.remove, view.removeDisplaced])
            compare(transition, null)
        for (let i = 0; i < view.count; i++) {
            compare(view.itemAtIndex(i).y, i * 60)
            compare(view.itemAtIndex(i).opacity, 1)
        }
    }
    function test_overlapping_edits_settle_to_the_latest_destination() {
        const view = createTemporaryObject(fixture, test)
        view.model = createTemporaryObject(resetModel, test)
        view.forceLayout()
        const kept = view.itemAtIndex(1)
        view.model.remove(0); view.forceLayout()
        view.model.insert(0, {title:"new"}); view.forceLayout()
        view.model.move(1, 0, 1); view.forceLayout()
        view.visible = false
        compare(view.itemAtIndex(0), kept)
        compare(kept.y, 0)
        compare(view.itemAtIndex(1).y, 60)
        wait(Motion.panel + 50)
        compare(kept.y, 0)
        compare(view.itemAtIndex(1).y, 60)
    }
    function test_cached_rows_outside_the_page_do_not_animate() {
        const page = createTemporaryObject(viewportFixture, test)
        const view = createTemporaryObject(fixture, page.contentItem, {motionViewport: page})
        view.model.append([{title:"a"}, {title:"b"}, {title:"c"}, {title:"d"}, {title:"e"}]); view.forceLayout()
        wait(Motion.panel + 50)
        const kept = view.itemAtIndex(3)
        view.model.move(3, 4, 1); view.forceLayout()
        compare(view.itemAtIndex(4), kept)
        compare(kept.y, 240)
        compare(view.itemAtIndex(3).y, 180)
        view.model.insert(3, {title:"offscreen"}); view.forceLayout()
        compare(view.itemAtIndex(3).opacity, 1)
        compare(view.itemAtIndex(3).y, 180)
        compare(kept.y, 300)
    }
    function test_initial_fill_and_reset_have_no_populate() {
        const view = createTemporaryObject(fixture, test)
        compare(view.populate, null)
        view.model = createTemporaryObject(resetModel, test)
        view.forceLayout()
        verify(!view.add || !view.add.running)
        view.model = createTemporaryObject(resetModel, test)
        view.forceLayout()
        compare(view.populate, null)
        verify(!view.add || !view.add.running)
    }
    function test_a_clipped_shelf_stops_and_reactivates_the_same_objects() {
        const page = createTemporaryObject(viewportFixture, test)
        const view = createTemporaryObject(fixture, page.contentItem, {motionViewport: page, y: 200})
        view.model.append([{title:"a"}, {title:"b"}]); view.forceLayout()
        const kept = view.itemAtIndex(0)
        compare(view.motionDuration, 0)
        view.model.insert(0, {title:"new"}); view.forceLayout()
        verify(!view.add || !view.add.running)
        page.contentY = 150
        compare(view.motionDuration, Motion.panel)
        view.model.move(1, 0, 1); view.forceLayout()
        verify(view.move.running)
        page.contentY = 0
        compare(view.motionDuration, 0)
        compare(view.move, null)
        page.contentY = 150
        compare(view.itemAtIndex(0), kept)
        compare(kept.y, 0)
    }
}
