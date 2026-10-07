import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as P

P.Modal {
    id: dialog
    required property var manager
    width: Math.min(Metrics.settingsMaxWidth, parent ? parent.width - Metrics.modalInsets : Metrics.confirmWidth)
    height: Math.min(Metrics.panesHeight, parent ? parent.height - Metrics.modalInsets : Metrics.panesHeight)
    ListView {
        anchors.top: parent.top
        anchors.bottom: close.top
        anchors.bottomMargin: Metrics.gap12
        width: parent.width
        clip: true
        model: dialog.manager.view.repositories || []
        spacing: Metrics.gap12
        ScrollBar.vertical: P.ViewportScrollBar { gutter: -Metrics.gap8;}
        delegate: Column {
            required property var modelData
            width: ListView.view.width - Metrics.gap12
            spacing: Metrics.gap8
            Text { text: modelData.url; textFormat: Text.PlainText; width: parent.width; wrapMode: Text.WrapAnywhere; color: Theme.color.textSecondary; font.pointSize: Type.body }
            Row {
                spacing: Metrics.gap8
                P.PillButton { text: I18n.tr("Refresh", I18n.rev); enabled: !dialog.manager.view.busy; onClicked: dialog.manager.command("refresh_repository", {id: modelData.id}) }
                P.PillButton { text: I18n.tr("Remove", I18n.rev); enabled: !dialog.manager.view.busy; onClicked: dialog.manager.command("remove_repository", {id: modelData.id}) }
            }
        }
    }
    P.PillButton { id: close; anchors.bottom: parent.bottom; anchors.right: parent.right; text: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
}
