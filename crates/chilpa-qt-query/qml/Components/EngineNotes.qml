import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Primitives"
import "../Theme"

// What the engine read on its own (F6): "A. Smith" → Composer: A Smith,
// "dnb" → Genre: Drum and Bass, and a genre widened to its near names
// (Rock + Rock & Roll). Each can go: the words are read as words, the genre
// exactly, and the query says so.
Rectangle {
    id: strip
    // The facade's notes: [{from, field, to, kind}], in its order.
    property var notes: []
    signal undone(int index)

    // Widened names of one genre read as one note.
    readonly property var shown: {
        const out = []
        notes.forEach((note, index) => {
            const same = note.kind === "near" ? out.find(o => o.kind === "near" && o.from === note.from) : null
            if (same) same.to = same.to + ", " + note.to
            else out.push({ from: note.from, field: note.field, to: note.to, kind: note.kind, index: index })
        })
        return out
    }
    visible: notes.length > 0
    implicitHeight: flow.implicitHeight + Refine.notesPadY * 2 // derived: padding above and below
    color: "transparent"
    Rectangle { width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }

    Flow {
        id: flow
        x: Refine.notesPadX
        y: Refine.notesPadY
        width: parent.width - Refine.notesPadX * 2 // derived: padding on both sides
        spacing: Refine.headApart
        Row {
            height: Refine.noteTall
            spacing: Refine.innerApart
            Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.infoCircle; size: Refine.rowButtonIcon; color: Theme.color.textSecondary }
            Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Read as", I18n.rev); font.pointSize: Type.small; font.weight: Type.semibold; color: Theme.color.textSecondary }
        }
        Repeater {
            model: strip.shown
            Rectangle {
                required property var modelData
                objectName: "engineNote"
                width: line.implicitWidth + Refine.valuePadLeft + Refine.valuePadRight
                height: Refine.noteTall
                radius: Refine.noteRound
                color: Theme.color.surfaceWindow
                border.width: Metrics.hairline
                border.color: Theme.color.strokeHairline
                Row {
                    id: line
                    x: Refine.valuePadLeft
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: Refine.innerApart
                    Text { anchors.verticalCenter: parent.verticalCenter; text: modelData.kind === "near" ? modelData.from : "“" + modelData.from + "”"; font.pointSize: Type.meta; color: Theme.color.textPrimary }
                    Icon { anchors.verticalCenter: parent.verticalCenter; glyph: modelData.kind === "near" ? Icons.plus : Icons.arrowRight; size: Refine.chipIcon; color: Theme.color.textTertiary }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: modelData.kind === "near" ? modelData.to : RefineText.field(modelData.field) + ": " + modelData.to; font.pointSize: Type.meta; font.weight: Type.medium; color: Theme.color.textPrimary }
                    RemoveDot {
                        anchors.verticalCenter: parent.verticalCenter
                        tint: Theme.color.textTertiary
                        tip: modelData.kind === "name" ? I18n.trArgs("Read “{}” as words instead", [modelData.from], I18n.rev) : I18n.trArgs("Match {} exactly", [modelData.from], I18n.rev)
                        onClicked: strip.undone(modelData.index)
                    }
                }
            }
        }
    }
}
