import QtQuick
import com.blitzfc.chilpa
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"
Row {
    property var about: ({})
    spacing: Metrics.gap8
    visible: !!about.next
    PillButton { text: I18n.tr("Next page", I18n.rev); onClicked: Navigation.goKey("source", parent.about.plugin + " native:" + parent.about.next) }
}
