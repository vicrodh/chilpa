import QtQuick

// Normal-mode row actions stay inside the active visual scope.
QtObject {
    required property var window
    property bool immersiveOpen: false
    function activeList(root) {
        if (!root || !root.visible || root.opacity === 0) return null
        if (root.viTrackList === true) {
            const point = root.mapToItem(window.contentItem, 0, 0)
            return root.width > 0 && root.height > 0 && point.x < window.contentItem.width && point.x + root.width > 0
                && point.y < window.contentItem.height && point.y + root.height > 0 ? root : null
        }
        for (let i = root.children.length - 1; i >= 0; --i) {
            const found = activeList(root.children[i])
            if (found) return found
        }
        return null
    }
    function focusedList() {
        if (immersiveOpen) return null
        for (let item = window.activeFocusItem; item; item = item.parent)
            if (item.viTrackList === true && item.visible) return item
        return activeList(window.contentItem)
    }
    function restoreFocus(previous) {
        if (previous && previous.visible) previous.forceActiveFocus()
        else { const list = focusedList(); if (list) list.forceActiveFocus() }
    }
}
