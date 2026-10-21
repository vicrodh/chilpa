import QtQuick
import "../Theme"
import "../Primitives"

// I keep progress resources on favorite controls and only animate an active write.
IconButton {
    id: button
    heart: true
    readonly property Item progressMark: progress.item
    Loader {
        id: progress
        anchors.centerIn: parent
        active: button.busy && button.visible
        sourceComponent: BusyMark {
            size: button.iconSize
            color: !button.enabled ? Theme.color.textTertiary
                 : button.tone === "solid" ? Theme.color.textOnAccent
                 : button.heart && button.checked ? Theme.color.favorite
                 : button.tone === "overlay" ? (button.checked ? Theme.color.accent : Theme.color.overlayInk)
                 : button.checked || button.tone === "accent" ? Theme.color.accent
                 : button.tone === "primary" ? Theme.color.textPrimary : Theme.color.textSecondary
        }
    }
}
