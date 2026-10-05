import QtQuick
import "../Theme"

Text {
    property bool warning: true
    visible: text !== ""
    wrapMode: Text.Wrap
    textFormat: Text.PlainText
    font.pointSize: Type.small
    color: warning ? Theme.color.statusWarning : Theme.color.statusPositive
}
