pragma Singleton
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The About window's numbers (QBZ's AboutModal): as wide and tall as the
// window allows up to its largest, 40 from each edge; a head row as tall as
// the logo; the labels of the build's facts in a column of their own.
QtObject {
    readonly property int widest: 920
    readonly property int tallest: 860
    readonly property int gutter: 80
    readonly property int head: 56
    readonly property int factLabel: 84
    readonly property int logoDecode: 112
    readonly property int updateWidth: Metrics.menuMaxWidth
    readonly property int updateDelay: 15000
    readonly property int updateInterval: 86400000
    readonly property int notificationDot: 6
}
