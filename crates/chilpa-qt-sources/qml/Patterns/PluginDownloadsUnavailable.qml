import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as P

Column {
    property bool loading: true
    signal back()
    spacing: Metrics.gap12
    Text {
        text: parent.loading ? I18n.tr("Loading…", I18n.rev) : I18n.tr("The download view is unavailable.", I18n.rev)
        color: Theme.color.textSecondary; font.pointSize: Type.body
    }
    P.PillButton { text: I18n.tr("Back to plugins", I18n.rev); onClicked: parent.back() }
}
