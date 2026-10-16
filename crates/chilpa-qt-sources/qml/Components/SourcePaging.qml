import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
Row {
    property var about: ({})
    spacing: Metrics.gap8
    visible: !!about.next
    PillButton { text: I18n.tr("Next page", I18n.rev); onClicked: Navigation.goKey("source", parent.about.plugin + " native:" + parent.about.next) }
}
