import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import "../Theme"

// One visual and spacing contract for every application modal.
Popup {
    id: root

    default property alias body: inset.data
    property real preferredWidth: Metrics.confirmWidth
    property real preferredHeight: implicitHeight
    property real heightRatio: Metrics.modalMaxHeightRatio
    readonly property real maximumHeight: parent ? parent.height * heightRatio : preferredHeight

    width: Math.max(0, Math.min(preferredWidth, parent ? Math.max(0, parent.width - Metrics.modalInsets) : preferredWidth))
    height: Math.max(0, Math.min(preferredHeight, maximumHeight))

    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    PopupInput { surface: root }
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnReleaseOutside

    // Overlay parenting must not leave a nested confirmation open after its owner.
    function closeOwnedPopups(item) {
        for (const child of item.data || []) {
            if (child instanceof Popup) child.close()
            else if (child instanceof Item) closeOwnedPopups(child)
        }
    }
    onClosed: closeOwnedPopups(inset)

    contentItem: Flickable {
        id: viewport
        objectName: "modalViewport"
        property bool blocksWindowShortcuts: true
        implicitWidth: inset.implicitWidth + Metrics.modalInsets
        implicitHeight: inset.implicitHeight + Metrics.modalInsets
        contentWidth: width
        contentHeight: Math.max(height, inset.implicitHeight + Metrics.modalInsets)
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentHeight > height
        clip: true
        ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8 }

        // A press in the modal outside its field takes the keyboard from it.
        FocusRelease {}

        Item {
            id: inset
            x: Metrics.modalInset
            y: Metrics.modalInset
            width: Math.max(0, viewport.width - Metrics.modalInsets)
            height: Math.max(0, viewport.height - Metrics.modalInsets)
            implicitWidth: childrenRect.width
            implicitHeight: childrenRect.height
        }
    }

    background: Item {
        Hold {}
        Shadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowElevatedY
            blur: Metrics.shadowElevatedBlur
            color: Theme.color.shadowElevated
        }
        Shadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowKeyY
            blur: Metrics.shadowKeyBlur
            color: Theme.color.shadowElevatedKey
        }
        Rectangle {
            id: panel
            anchors.fill: parent
            radius: Metrics.radiusPanel
            color: Theme.color.surfaceRaised
            Edge {}
        }
    }
}
