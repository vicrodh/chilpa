import QtQuick
import "../Theme"
import "../Primitives"

// "Sort: Artist", opening the orders. The one in use carries a caret for
// its direction, and choosing it again turns the direction around, as in
// QBZ's sidebar: no order has a "reverse" of its own. `options` is a list
// of { value, text }, with `descending: true` on an order whose own
// direction is the newest or the most first and `fixed: true` on one that
// has no direction. `value` names the order and its direction at once: the
// order alone in its own direction, with "-desc" or "-asc" the other way.
// Choices that are not orders (a grouping, a size) set `directed: false`
// and keep their check.
Item {
    id: sort

    property var options: []
    property string value
    property bool directed: true
    property string glyph: Icons.arrowsSort

    signal picked(string value)

    readonly property string base: baseOf(value)
    readonly property int current: options.findIndex((o) => o.value === base)
    readonly property bool turnable: directed && current >= 0 && options[current].fixed !== true
    readonly property bool descending: turnable && (options[current].descending === true) !== (value !== base)

    // The order a name with its direction stands for.
    function baseOf(name) {
        return directed ? String(name).replace(/-(asc|desc)$/, "") : name
    }
    // The order `name` the other way round from its own direction.
    function turned(name) {
        const own = options.find((o) => o.value === name)
        return name + (own && own.descending === true ? "-asc" : "-desc")
    }

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
        objectName: "sortMenu"
        searchable: sort.options.length > 5
        y: pill.height + Metrics.gap4
        x: pill.width - width
        entries: sort.options.map((o, i) => i === sort.current && sort.turnable
            ? { text: o.text, glyph: sort.descending ? Icons.chevronDown : Icons.chevronUp, accent: true }
            : { text: o.text, checked: i === sort.current })
        onChosen: (index) => {
            const chosen = sort.options[index].value
            sort.picked(index === sort.current && sort.turnable && sort.value === sort.base ? sort.turned(chosen) : chosen)
        }
    }
}
