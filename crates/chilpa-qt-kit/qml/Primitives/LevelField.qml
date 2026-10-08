import QtQuick
import "../Theme"

// A level from 0 to 100 with its percentage beside it (design/03
// SettingsRow, slider). The owner tells apart what the slider shows while
// it is dragged from what is kept when it is let go.
Row {
    id: field

    property int value: 0
    // What the percentage says: the value, or what the owner previews.
    property string shown: String(value)
    property string tip

    signal moved(int value)
    signal released(int value)

    spacing: Metrics.gap12

    LevelSlider {
        anchors.verticalCenter: parent.verticalCenter
        value: field.value
        tip: field.tip
        onMoved: (level) => field.moved(level)
        onReleased: (level) => field.released(level)
    }
    Text {
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.settingsPercent
        horizontalAlignment: Text.AlignRight
        text: field.shown + "%"
        font.pointSize: Type.row
        font.features: { "tnum": 1 }
        color: Theme.color.textSecondary
    }
}
