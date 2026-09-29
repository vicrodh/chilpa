import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import "../Theme"

// A menu of entries (design/03 Menu). Each entry is
// { text, detail, checked, mark, hint, separator, submenu }: a separator
// draws a line and nothing else; `mark` is a short badge ("BP") in the accent.
// Chosen by click; closed by a click outside or Esc.
Popup {
    id: menu

    // A click outside closes it and is nothing else: it is not also a click
    // on what is there.
    modal: true
    dim: false

    property var entries: []
    property string title
    property real widthLimit: Metrics.menuMaxWidth

    signal chosen(int index)
    signal submenu(int index, real top)

    // As wide as its widest entry, between the design's width and a limit;
    // past the limit an entry is cut and says the rest in a tooltip. Kept
    // inside the window.
    property real widest: 0

    function measure() {
        let w = 0
        for (let i = 0; i < entriesRepeater.count; ++i) {
            const item = entriesRepeater.itemAt(i)
            if (item)
                w = Math.max(w, item.implicitWidth)
        }
        widest = Math.max(w, titleLabel.implicitWidth + Metrics.gap16)
    }

    padding: Metrics.gap4
    margins: Metrics.gap10
    implicitWidth: Math.min(widthLimit, Math.max(Metrics.menuWidth, widest + padding * 2)) // derived: padding on both sides

    background: Item {
        Hold {}
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowElevatedY
            blur: Metrics.shadowElevatedBlur
            color: Theme.color.shadowElevated
        }
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowKeyY
            blur: Metrics.shadowKeyBlur
            color: Theme.color.shadowElevatedKey
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

        Text {
            id: titleLabel
            visible: menu.title !== ""
            width: column.width
            height: visible ? Metrics.menuItem : 0
            leftPadding: Metrics.gap10
            rightPadding: Metrics.gap10
            verticalAlignment: Text.AlignVCenter
            text: menu.title
            elide: Text.ElideRight
            font.pointSize: Type.small
            font.weight: Type.semibold
            color: Theme.color.textTertiary
            onImplicitWidthChanged: menu.measure()
        }

        Repeater {
            id: entriesRepeater
            model: menu.entries
            onItemAdded: menu.measure()
            onItemRemoved: menu.measure()

            MenuEntry {
                required property int index
                required property var modelData
                width: column.width
                entry: modelData
                onImplicitWidthChanged: menu.measure()
                // Closed before it says what was chosen: what it says can
                // rebuild the view the menu lives in.
                onClicked: {
                    if (modelData.submenu) {
                        menu.submenu(index, menu.y + y)
                        return
                    }
                    menu.close()
                    menu.chosen(index)
                }
            }
        }
    }
}
