import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Primitives"
import "../Theme"

// A row of presets after a lead word (F3): "Added in the last" any time,
// 7 days, 30 days, 6 weeks, 3 months, 1 year; and how often a track was
// played. The one the query holds is inverted.
Flow {
    id: presets
    property string lead: ""
    // [{text, on, change}]
    property var choices: []
    signal edit(var change)
    spacing: Refine.innerApart
    Text {
        visible: presets.lead !== ""
        height: Refine.pillTall
        verticalAlignment: Text.AlignVCenter
        rightPadding: Refine.segTrackPad
        text: presets.lead
        font.pointSize: Type.row
        color: Theme.color.textSecondary
    }
    Repeater {
        model: presets.choices
        Pill {
            required property var modelData
            text: modelData.text
            on: modelData.on
            onClicked: if (modelData.change) presets.edit(modelData.change)
        }
    }
}
