import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// "Sort: Artist", opening the orders with a check on the one in use.
// `options` is a list of { value, text }.
Item {
    id: sort

    property var options: []
    property string value
    property string glyph: Icons.arrowsSort

    signal picked(string value)

    readonly property int current: options.findIndex((o) => o.value === value)

    implicitWidth: pill.implicitWidth
    implicitHeight: pill.implicitHeight

    PillButton {
        id: pill
        glyph: sort.glyph
        text: sort.current >= 0 ? sort.options[sort.current].text : ""
        onClicked: menu.open()
    }

    Menu {
        id: menu
        y: pill.height + Metrics.gap4
        x: pill.width - width
        entries: sort.options.map((o, i) => ({ text: o.text, checked: i === sort.current }))
        onChosen: (index) => sort.picked(sort.options[index].value)
    }
}
