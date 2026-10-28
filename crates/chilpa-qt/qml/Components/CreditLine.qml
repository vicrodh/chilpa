import QtQuick
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

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
    readonly property var document: visible && credit !== "" && parentSource === "" && (trackPath !== "" || albumId < 0) && typeof Sources.creditDocument === "function"
        ? JSON.parse(Sources.creditDocument(albumId, trackPath, Library.remoteRev) || "{}") : ({})

    // Which of its artists is at `x`: the first is 0, and between two of
    // them there is none.
    function placeAt(x, credit = line.credit) {
        const stated = Navigation.creditStops(credit)
        const stops = stated === "" ? [] : stated.split(",").map(Number)
        for (let i = 0; i + 1 < stops.length; i += 2) {
            if (x >= metrics.advanceWidth(credit.slice(0, stops[i])) && x <= metrics.advanceWidth(credit.slice(0, stops[i + 1])))
                return i / 2
        }
        return stops.length === 0 ? 0 : -1
    }

    function openCredit(target, x) {
        if (target.person) { Navigation.goKey(target.person.target, target.person.key); return }
        if (target.parentSource !== "") {
            Sources.command("playing_parent", "", "")
            Navigation.goKey("source", target.parentSource)
            return
        }
        const route = Sources.artistKey(target.albumId, target.trackPath)
        if (route) Navigation.goKey("source", route)
        else if (target.entry !== "") Navigation.goKey("sourceEntry", target.entry)
        else Navigation.openCredit(target.credit, line.placeAt(x, target.credit))
    }

    text: document.html || credit
    // Styled text keeps the link color and elides; rich text does neither.
    textFormat: document.links ? Text.StyledText : Text.PlainText
    linkColor: color
    elide: Text.ElideRight

    FontMetrics { id: metrics; font: line.font }
    HoverHandler { id: hover; enabled: line.credit !== ""; cursorShape: Qt.PointingHandCursor }
    TapHandler { enabled: line.credit !== ""; onTapped: (point) => {
        const link = line.document.links ? line.linkAt(point.position.x, point.position.y) : ""
        if (line.document.links && link === "") return
        const person = link !== "" ? line.document.links[Number(link)] : null
        const target = {person: person, parentSource: line.parentSource, albumId: line.albumId, trackPath: line.trackPath, entry: line.entry, credit: line.credit}
        const window = line.Window.window
        if (typeof window?.deferNavigation === "function") window.deferNavigation(line.openCredit, target, point.position.x)
        else line.openCredit(target, point.position.x)
    } }
}
