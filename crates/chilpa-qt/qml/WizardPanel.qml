// The high fidelity wizard.
//
// It is a diagnosis, not a configuration: it measures the chosen output and
// says, format by format, whether what would come out is the same as what
// went in. It turns nothing off and writes no settings. QBZ's one wrote a
// list of assumed rates, and that is worse than not knowing.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ColumnLayout {
    id: root
    spacing: 8

    function fields(line) {
        return line.split("\t")
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 10
        Button {
            text: Chilpa.translate("Measure the output", Chilpa.trRev)
            onClicked: Chilpa.runWizard()
        }
        Label {
            Layout.fillWidth: true
            text: Chilpa.wizardSummary
            elide: Text.ElideRight
            font.pixelSize: 12
            color: "#411d1f"
        }
    }

    Label {
        visible: Chilpa.wizardRows.length === 0
        text: Chilpa.translate("Press measure to see what your output can do", Chilpa.trRev)
        color: "#7a411d1f"
    }

    ListView {
        id: list
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: Chilpa.wizardRows

        delegate: Item {
            width: list.width
            height: 26

            readonly property var cols: root.fields(modelData)
            readonly property bool perfect: cols[2] === "bit-perfect"
            readonly property bool impossible: cols[2] === Chilpa.translate("cannot be done", Chilpa.trRev)

            RowLayout {
                anchors.fill: parent
                anchors.rightMargin: 6
                spacing: 10
                Label {
                    Layout.preferredWidth: 140
                    text: cols[0]
                    elide: Text.ElideRight
                    font.pixelSize: 12
                    color: "#411d1f"
                }
                Label {
                    Layout.fillWidth: true
                    text: cols[1]
                    elide: Text.ElideRight
                    font.pixelSize: 11
                    color: "#7a411d1f"
                }
                Label {
                    text: cols[2]
                    font.pixelSize: 11
                    font.weight: perfect ? Font.DemiBold : Font.Normal
                    color: perfect ? "#2e7d32" : (impossible ? "#d7272b" : "#7a411d1f")
                }
            }
        }
    }
}
