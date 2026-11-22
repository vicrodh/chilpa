import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: field
    property string value: ""
    spacing: Metrics.gap6
    Clipboard { id: clipboard }
    Text {
        width: parent.width
        text: field.value
        textFormat: Text.PlainText
        wrapMode: Text.WrapAnywhere
        color: Theme.color.textSecondary
        font.pointSize: Type.body
    }
    PillButton {
        text: I18n.tr("Copy", I18n.rev)
        onClicked: clipboard.write(field.value)
    }
}
