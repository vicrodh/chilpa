// The audio stack: what the samples go through before they are sound.
//
// It comes before the outputs because it decides them -- which exist, what
// they are called, and whether bit-perfect is on the table at all. A machine
// offers several and they are not interchangeable: one hands the samples to
// the DAC as they left the file, another mixes them with a notification and
// resamples the result.
//
// Each row is "key, name, present, missing, fidelity, caveat" from the
// bridge. One that is not on this machine stays in the list, greyed, with
// what is missing: leaving it out teaches nobody anything, and what is
// missing is usually one package away.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ComboBox {
    id: box

    model: Chilpa.stacks

    function fields(line) {
        return (line === undefined ? "" : line).split("\t")
    }
    function here(line) {
        return box.fields(line)[2] === "1"
    }

    currentIndex: {
        for (let i = 0; i < box.model.length; i++)
            if (box.fields(box.model[i])[0] === Chilpa.stack) return i
        return 0
    }

    displayText: box.fields(box.model[box.currentIndex])[1]

    onActivated: {
        const row = box.fields(box.model[index])
        // One that is not here cannot be chosen: it would leave the window
        // showing a stack with no outputs and no way back.
        if (row[2] === "1") Chilpa.chooseStack(row[0])
        else box.currentIndex = box.currentIndex
    }

    delegate: ItemDelegate {
        width: box.width
        enabled: box.here(modelData)
        highlighted: box.highlightedIndex === index

        readonly property var cols: box.fields(modelData)

        contentItem: ColumnLayout {
            spacing: 0
            Label {
                Layout.fillWidth: true
                text: cols[1]
                elide: Text.ElideRight
                font.pixelSize: 13
                color: box.here(modelData) ? "#411d1f" : "#7a411d1f"
            }
            // What is missing, or what it cannot promise. Never both: one
            // that is not here promises nothing at all.
            Label {
                Layout.fillWidth: true
                visible: text !== ""
                text: box.here(modelData) ? "" : (cols[3] === undefined ? "" : cols[3])
                wrapMode: Text.Wrap
                font.pixelSize: 10
                color: "#7a411d1f"
            }
        }
    }
}
