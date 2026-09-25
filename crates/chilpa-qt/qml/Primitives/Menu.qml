import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import "../Theme"

// A menu of entries (design/03 Menu). Each entry is
// { text, detail, checked, mark, hint, separator }: a separator draws a
// line and nothing else; `mark` is a short badge ("BP") in the accent.
// Chosen by click; closed by a click outside or Esc.
Popup {
    id: menu

    property var entries: []

    signal chosen(int index)

    padding: Metrics.gap4
    implicitWidth: Math.max(Metrics.menuWidth, column.implicitWidth + padding * 2) // derived: padding on both sides

    background: Item {
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowElevatedY
            blur: Metrics.shadowElevatedBlur
            color: Theme.color.shadowElevated
        }
        Rectangle {
            id: panel
            anchors.fill: parent
            radius: Metrics.radiusMenu
            color: Theme.color.surfaceRaised

            Edge {}
        }
    }

    contentItem: Column {
        id: column

        Repeater {
            model: menu.entries

            MenuEntry {
                required property int index
                required property var modelData
                width: column.width
                entry: modelData
                // Closed before it says what was chosen: what it says can
                // rebuild the view the menu lives in.
                onClicked: {
                    menu.close()
                    menu.chosen(index)
                }
            }
        }
    }
}
