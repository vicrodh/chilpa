// The output picker: what a person recognises, and whether it is direct.
//
// One combo box used in both places an output is chosen, so the two cannot
// drift. Each row carries three fields from the bridge, tab separated: the
// name, the line under it, and whether samples reach it untouched.
//
// Why the name and not the device: every USB DAC in the world calls its
// device "USB Audio", so a list built on device names shows a DAC, a dock
// and a webcam as three lines that read the same. The card's name is the
// one somebody recognises, and the "BP" mark says which of them is a direct
// path rather than something that converts on the way.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ComboBox {
    id: box

    // The rows as they come: "name\tdetail\tbit-perfect".
    model: Chilpa.outputs

    function fields(line) {
        return (line === undefined ? "" : line).split("\t")
    }

    // What is chosen lives in the bridge; this only shows it. That way the
    // list stays put even if something else changes the output.
    currentIndex: {
        const i = Chilpa.outputIds.indexOf(Chilpa.device)
        return i >= 0 ? i : 0
    }

    displayText: box.fields(box.model[box.currentIndex])[0]

    delegate: ItemDelegate {
        width: box.width
        highlighted: box.highlightedIndex === index

        readonly property var cols: box.fields(modelData)

        contentItem: RowLayout {
            spacing: 8

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                Label {
                    Layout.fillWidth: true
                    text: cols[0]
                    elide: Text.ElideRight
                    font.pixelSize: 13
                    color: "#411d1f"
                }
                Label {
                    Layout.fillWidth: true
                    visible: cols[1] !== undefined && cols[1] !== ""
                    text: cols[1] === undefined ? "" : cols[1]
                    elide: Text.ElideMiddle
                    font.pixelSize: 10
                    color: "#7a411d1f"
                }
            }

            // The mark, not a word: it sits beside a name and has to stay
            // out of the way of the name.
            Rectangle {
                visible: cols[2] === "1"
                Layout.preferredWidth: mark.implicitWidth + 10
                Layout.preferredHeight: 18
                radius: 4
                color: "#1a2e7d32"
                Label {
                    id: mark
                    anchors.centerIn: parent
                    text: "BP"
                    font.pixelSize: 10
                    font.weight: Font.DemiBold
                    color: "#2e7d32"
                }
            }
        }
    }
}
