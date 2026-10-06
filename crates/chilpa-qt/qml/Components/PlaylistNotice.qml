import QtQuick
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"

Text {
    property bool warning: true
    visible: text !== ""
    wrapMode: Text.Wrap
    textFormat: Text.PlainText
    font.pointSize: Type.small
    color: warning ? Theme.color.statusWarning : Theme.color.statusPositive
}
