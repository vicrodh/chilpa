import QtQuick
import "../Theme"

// The area under the crumb line: the columns or Refine, one at a time, only
// the one shown alive. Refine sizes to its content up to the height the grip
// gives (`wanted`); the columns take all of it.
Loader {
    id: area
    property bool refineMode: false
    property Component columns
    property string summary: ""
    property bool narrow: false
    readonly property real wanted: refineMode && item ? item.implicitHeight : height
    // While a field's values or the suggestions are open the panel may pass
    // the grip's height, as far as the design's 1000.
    readonly property bool opened: refineMode && !!item && (item.picking !== "" || item.typing)
    function fit(grip) { return Math.min(opened ? Refine.pickGrow : grip, wanted) }
    active: visible
    sourceComponent: refineMode ? refine : columns
    Component {
        id: refine
        RefinePanel {
            summary: area.summary
            narrow: area.narrow
        }
    }
}
