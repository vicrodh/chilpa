import QtQuick
import "../Theme"
import "../Theme/Viewport.js" as Viewport
// I own six transitions only while visible and motion is enabled.
ListView {
    id: view
    property Flickable motionViewport: null
    readonly property bool motionVisible: visible && (!motionViewport || Viewport.topIn(view, motionViewport.contentItem) + height > motionViewport.contentY && Viewport.topIn(view, motionViewport.contentItem) < motionViewport.contentY + motionViewport.height)
    readonly property int motionDuration: motionVisible ? Motion.panel : 0
    readonly property int arrivalDuration: motionVisible ? Motion.base : 0
    function inViewport(item, point) {
        const x = point.x - contentX, y = point.y - contentY
        if (x + item.width <= 0 || x >= width || y + item.height <= 0 || y >= height) return false
        const top = motionViewport ? Viewport.topIn(view, motionViewport.contentItem) + y : 0
        return !motionViewport || top + item.height > motionViewport.contentY && top < motionViewport.contentY + motionViewport.height
    }
    function rowVisible(item, point) {
        return item && (inViewport(item, Qt.point(item.x, item.y)) || inViewport(item, point))
    }
    function updateMotion() {
        const targets = bank.item ? Array.from(bank.item.targets) : []
        bank.active = motionDuration > 0
        if (!bank.active) {
            for (const [item, point] of targets) {
                if (item && item.parent) { item.x = point.x; item.y = point.y; item.opacity = item.motionOpacity ?? 1 }
            }
            forceLayout()
        }
    }
    // Destroying the templates cancels Qt's cloned jobs; I settle their rows.
    onMotionDurationChanged: updateMotion()
    Component.onCompleted: updateMotion()
    add: bank.item ? bank.item.arrival : null
    addDisplaced: bank.item ? bank.item.shiftAdd : null
    move: bank.item ? bank.item.reorder : null
    moveDisplaced: bank.item ? bank.item.shiftMove : null
    remove: bank.item ? bank.item.depart : null
    removeDisplaced: bank.item ? bank.item.shiftRemove : null
    // Populate and model reset intentionally have no transition.
    Loader {
        id: bank
        active: false
        sourceComponent: QtObject {
            id: motions
            // I retain at most the planner's 512 destinations until jobs finish.
            property var targets: new Map()
            function remember(item, point) {
                targets.delete(item)
                if (targets.size === 512) targets.delete(targets.keys().next().value)
                targets.set(item, Qt.point(point.x, point.y))
            }
            function releaseFinished() {
                if (![arrival, shiftAdd, reorder, shiftMove, shiftRemove].some(t => t && t.running)) targets.clear()
            }
            component Slide: Transition {
                id: shift
                property bool fading: false
                onRunningChanged: if (!running) motions.releaseFinished()
                SequentialAnimation {
                    ScriptAction { script: motions.remember(shift.ViewTransition.item, shift.ViewTransition.destination) }
                    ParallelAnimation {
                        NumberAnimation { properties: "x,y"; duration: !shift.fading && view.rowVisible(shift.ViewTransition.item, shift.ViewTransition.destination) ? view.motionDuration : 0; easing.type: Easing.OutCubic }
                        NumberAnimation { property: shift.fading ? "opacity" : ""; from: 0; to: shift.ViewTransition.item?.motionOpacity ?? 1; duration: shift.fading && shift.ViewTransition.item && view.inViewport(shift.ViewTransition.item, shift.ViewTransition.destination) ? view.arrivalDuration : 0 }
                    }
                }
            }
            property Transition arrival: Slide { fading: true }
            property Transition shiftAdd: Slide {}
            property Transition reorder: Slide {}
            property Transition shiftMove: Slide {}
            property Transition shiftRemove: Slide {}
            property Transition depart: Transition {
                id: departure
                NumberAnimation {
                    properties: view.orientation === ListView.Horizontal ? "x" : "y"
                    to: -(view.orientation === ListView.Horizontal ? departure.ViewTransition.item?.width || 0 : departure.ViewTransition.item?.height || 0)
                    duration: departure.ViewTransition.item && view.inViewport(departure.ViewTransition.item, Qt.point(departure.ViewTransition.item.x, departure.ViewTransition.item.y)) ? view.motionDuration : 0
                    easing.type: Easing.InCubic
                }
            }
        }
    }
}
