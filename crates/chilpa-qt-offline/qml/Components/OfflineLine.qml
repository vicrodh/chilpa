import QtQuick
import app.chilpa.kit
import app.chilpa.offline
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// A page's header rule that also says how a selection made available
// offline from it goes: the hairline as always (or nothing, with `rule`
// off) and, while the selection's tracks arrive, a thin line over it that
// grows with them. It keeps the rule's height, so nothing under it moves.
Rectangle {
    id: line
    // `album`, `playlist` or `tracks`, and the selection's ids as JSON;
    // empty ids take the last selection of the kind.
    property string kind: ""
    property string selected: ""
    property bool rule: true
    readonly property real progress: kind === "" ? -1 : Offline.job(kind, selected, Offline.rev)

    height: Metrics.hairline
    color: rule ? Theme.color.strokeHairline : "transparent"

    Rectangle {
        objectName: "offlineLineFill"
        visible: line.progress >= 0
        y: (line.height - height) / 2 // derived: centred on the rule
        // Never less than a ring's width: a selection that just began shows.
        width: Math.min(line.width, Math.max(Metrics.offlineRing, line.width * line.progress))
        height: Metrics.offlineLine
        radius: height / 2 // derived: round ends
        color: Theme.color.accent
    }
}
