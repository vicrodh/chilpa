import QtQuick
import QtQuick.Controls.Basic
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// I keep the pair open across the overlap and while either panel is hovered.
Menu {
    id: root
    modal: false
    focus: true
    signal actionChosen(var entry)
    readonly property bool pairHovered: hovering || branch.hovering
    property int branchIndex: -1
    function showBranch(index, top) {
        if (!entries[index] || entries[index].enabled === false) return
        const children = entries[index].children
        if (!children || !children.length) return
        branch.entries = children
        branch.measure()
        const overlay = Overlay.overlay
        const origin = contentItem.mapToItem(overlay, 0, 0)
        const right = origin.x + availableWidth - Metrics.gap4
        const left = origin.x - branch.width + Metrics.gap4
        branch.x = Math.max(Metrics.gap10, right + branch.width <= overlay.width - Metrics.gap10 ? right : left) - origin.x
        branch.heightLimit = overlay.height - Metrics.gap10 * 2 // derived: top and bottom margins
        branch.y = Math.max(Metrics.gap10, Math.min(origin.y + top, overlay.height - branch.height - Metrics.gap10)) - origin.y
        branchIndex = index
        leave.stop()
        branch.open()
    }
    onEntryHovered: (index, top) => { if (entries[index].submenu) showBranch(index, top) }
    onSubmenu: (index, top) => showBranch(index, top - y)
    onChosen: (index) => actionChosen(entries[index])
    onClosed: { leave.stop(); branch.close(); branchIndex = -1 }
    onPairHoveredChanged: {
        if (pairHovered) leave.stop()
        else if (branch.visible) leave.restart()
    }
    closePolicy: Popup.CloseOnEscape
    // A shared backdrop consumes outside clicks without blocking the other popup.
    // It hangs from the overlay only while the menu is open: left there hidden,
    // it still swallowed clicks on the area the menu had covered.
    // no cursor: the backdrop only blocks clicks on the page beneath it.
    MouseArea {
        objectName: "menuBackdrop"
        parent: root.visible ? root.Overlay.overlay : null
        anchors.fill: parent
        visible: root.visible
        z: -1
        acceptedButtons: Qt.AllButtons
        onClicked: root.close()
        onWheel: (wheel) => wheel.accepted = true
    }
    Timer { id: leave; interval: Motion.menuLeave; onTriggered: if (!root.pairHovered) root.close() }
    Menu {
        id: branch
        objectName: "menuBranch"
        parent: root.contentItem
        modal: false
        focus: true
        closePolicy: Popup.CloseOnEscape
        onClosed: root.close()
        onChosen: (index) => root.actionChosen(entries[index])
    }
}
