import QtQuick
import "../Theme"
import "../Components"

// The area under the crumb line: the columns or Refine, one at a time, only
// the one shown alive. Refine sizes to its content up to the height the grip
// gives (`wanted`); the columns take all of it.
Loader {
    id: area
    property bool refineMode: false
    property Component columns
    property string summary: ""
    property bool narrow: width < Refine.narrowBelow
    // Never the area's own height: that is fit(), which reads this.
    readonly property real wanted: refineMode && item ? item.implicitHeight : 0
    // While a field's values or the suggestions are open the panel may pass
    // the grip's height, as far as the design's 1000.
    readonly property bool opened: refineMode && !!item && !detached && (item.picking !== "" || item.typing)
    function fit(grip) { return detached ? Refine.dockedTall : Math.min(opened ? Refine.pickGrow : grip, wanted) }
    // Refine's own grip height, kept apart from the columns' height.
    readonly property int gripStart: Refine.gripStart
    function gripClamp(height, room) { return Math.max(Refine.gripLeast, Math.min(room - Refine.gripRoom, height)) }
    // Refine in its own window; the area keeps a line to dock it back.
    property bool detached: false
    active: visible
    sourceComponent: refineMode ? (detached ? docked : refine) : columns
    Component {
        id: refine
        RefinePanel {
            summary: area.summary
            narrow: area.narrow
            onDetachAsked: area.detached = true
        }
    }
    Component { id: docked; DockedNote { onDockAsked: area.detached = false } }
    Loader {
        objectName: "refineWindowLoader"
        active: area.refineMode && area.detached
        sourceComponent: RefineWindow {
            summary: area.summary
            transientParent: area.Window.window
            onDockAsked: area.detached = false
        }
    }
}
