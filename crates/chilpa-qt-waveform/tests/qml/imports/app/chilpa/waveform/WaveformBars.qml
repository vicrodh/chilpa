import QtQuick

// The native bars item (cxx/waveform_bars.cpp) as the tests see it: the same
// geometry and colors in script, readable through the same two functions.
Item {
    property var values: []
    property var lows: []
    property real played: 0
    property real analyzed: 0
    property real slot: 1
    property real barWide: 1
    property real halfTall: 1
    property real barLeast: 1
    property color playedColor
    property color knownColor
    property color restColor
    readonly property int count: values.length === lows.length ? values.length : 0
    function barAt(index) {
        const least = barLeast / 2
        const room = Math.max(0, halfTall - least)
        const reach = Math.min(halfTall, least + Math.min(1, Math.max(0, values[index])) * room)
        const drop = Math.min(halfTall, least + Math.min(1, Math.max(0, lows[index])) * room)
        return Qt.rect(slot * index, height / 2 - reach, barWide, reach + drop)
    }
    function colorAt(index) {
        if ((index + 1) / count <= played) return playedColor
        return index / count < analyzed ? knownColor : restColor
    }
}
