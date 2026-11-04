import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The way on and back of adding or editing a server (ServerWizard): Back
// while there is a step behind, Cancel until it is done, and the step's own
// action. The wizard says what each one is and does.
Item {
    id: footer
    property var wizard: ({})
    readonly property var setup: wizard.setup || ({})
    height: Metrics.pillHeight
    PillButton {
        objectName: "serverBack"
        visible: footer.wizard.page > (footer.wizard.editing ? 1 : 0) && !footer.wizard.done
        anchors.left: parent.left
        text: I18n.tr("Back", I18n.rev)
        enabled: !footer.wizard.busy || footer.setup.searching === true || footer.wizard.step === "code"
        onClicked: footer.wizard.back()
    }
    Row {
        anchors.right: parent.right
        spacing: Metrics.gap8
        PillButton { visible: !footer.wizard.done; text: I18n.tr("Cancel", I18n.rev); onClicked: footer.wizard.close() }
        PillButton {
            objectName: "serverContinue"
            kind: "primary"
            text: footer.wizard.primaryText || ""
            enabled: !!footer.wizard.primaryEnabled
            onClicked: footer.wizard.primary()
        }
    }
}
