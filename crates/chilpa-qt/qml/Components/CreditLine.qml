import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "../Theme"

// A credit as its tag wrote it. It names one artist or several, and a click
// opens the page of the one under the pointer.
Text {
    id: line
    property double albumId: 0
    property string trackPath: ""
    property string credit
    // When what plays is an entry of a source, the key of its page: a
    // station is not an artist, and a click opens the station.
    property string parentSource
    property string entry
    readonly property alias hovered: hover.hovered

    // Which of its artists is at `x`: the first is 0, and between two of
    // them there is none.
    function placeAt(x) {
        const stated = Navigation.creditStops(credit)
        const stops = stated === "" ? [] : stated.split(",").map(Number)
        for (let i = 0; i + 1 < stops.length; i += 2) {
            if (x >= metrics.advanceWidth(credit.slice(0, stops[i])) && x <= metrics.advanceWidth(credit.slice(0, stops[i + 1])))
                return i / 2
        }
        return stops.length === 0 ? 0 : -1
    }

    text: credit
    elide: Text.ElideRight

    FontMetrics { id: metrics; font: line.font }
    HoverHandler { id: hover; enabled: line.credit !== ""; cursorShape: Qt.PointingHandCursor }
    TapHandler { enabled: line.credit !== ""; onTapped: (point) => {
        if (line.parentSource !== "") {
            Sources.command("playing_parent", "", "")
            Navigation.goKey("source", line.parentSource)
            return
        }
        const route = Sources.artistKey(line.albumId, line.trackPath)
        if (route) Navigation.goKey("source", route)
        else if (line.entry !== "") Navigation.goKey("sourceEntry", line.entry)
        else Navigation.openCredit(line.credit, line.placeAt(point.position.x))
    } }
}
