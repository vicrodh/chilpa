import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// A plugin of the third-party catalog says so beside its name: others
// publish it, not Chilpa. `by` names the GitHub account that does, when
// the catalog says where the plugin lives.
Rectangle {
    id: tag
    objectName: "pluginThirdParty"
    property string by: ""
    implicitWidth: label.implicitWidth + Metrics.gap8 * 2 // derived: padding on both sides
    implicitHeight: label.implicitHeight + Metrics.gap2 * 2 // derived: padding above and below
    radius: height / 2 // derived: a pill
    color: Theme.color.fillField
    Text {
        id: label
        anchors.centerIn: parent
        text: tag.by ? I18n.trArgs("Third-party by {}", [tag.by], I18n.rev) : I18n.tr("Third-party", I18n.rev)
        font.pointSize: Type.micro
        font.weight: Type.medium
        color: Theme.color.textSecondary
    }
}
