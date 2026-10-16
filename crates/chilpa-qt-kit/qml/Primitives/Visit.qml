pragma Singleton
import QtQuick

// What a screen was doing when it was left, for when it is come back to:
// what was typed in it, how far it was scrolled. It lasts as long as the
// window; what should outlast it is a preference, and is kept there.
QtObject {
    property var kept: ({})

    function recall(name, otherwise) {
        return kept[name] === undefined ? otherwise : kept[name]
    }
    function keep(name, value) {
        kept[name] = value
    }
}
