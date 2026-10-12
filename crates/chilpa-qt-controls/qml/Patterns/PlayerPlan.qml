import QtQuick
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// Where the player sits and where the planes beside it start and end, in
// the three layouts its View menu offers: floating over the content, as it
// always did, or a bar the window's width at the bottom or at the top
// (the owner, 2026-10-12). Docked, the sidebar, the content and the right
// panel stop at the bar instead of passing under it, and the bar stays put.
Item {
    id: plan

    required property Item window
    // The content's plane: the floating bar spans it, less the window's gap.
    required property Item content
    // How tall the bar stands now, and whether a still pointer sent it away.
    property real tall: Metrics.playerHeight
    property bool away: false
    // The window's own plan tells the planes where they go; a sample does not.
    property bool shapesWindow: true

    readonly property bool docked: PlayerLayout.mode === "bottom" || PlayerLayout.mode === "top"
    readonly property bool onTop: PlayerLayout.mode === "top"

    readonly property real planeTop: onTop ? Metrics.tabBarHeight + tall + Metrics.spaceWindow : Metrics.tabBarHeight // derived: under the bar and its gap
    // What the planes leave free at the bottom; the content's own plane
    // passes under a floating bar to the window's edge.
    readonly property real planeBottom: docked && !onTop ? tall + Metrics.spaceWindow * 2 : Metrics.spaceWindow // derived: the bar and a gap on each side
    readonly property real contentBottom: docked && !onTop ? planeBottom : (Theme.ambient ? Metrics.spaceWindow : 0)

    readonly property real barX: docked ? Metrics.spaceWindow : content.x + Metrics.spaceWindow
    readonly property real barWidth: (docked ? window.width : content.width) - Metrics.spaceWindow * 2 // derived: the gap on each side
    readonly property real barY: onTop ? Metrics.tabBarHeight
        : docked ? window.height - tall - Metrics.spaceWindow // derived: its gap above the window's edge
        : away ? window.height : content.y + content.height - tall - Metrics.spaceWindow

    Binding { target: Metrics; property: "shellTop"; value: plan.planeTop; when: plan.shapesWindow }
    Binding { target: Metrics; property: "shellBottom"; value: plan.planeBottom; when: plan.shapesWindow }
}
