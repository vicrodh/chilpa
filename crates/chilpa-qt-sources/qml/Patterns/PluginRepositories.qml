import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as P

P.Modal {
    id: dialog
    objectName: "pluginRepositories"
    required property var manager
    width: Math.min(Metrics.settingsMaxWidth, parent ? parent.width - Metrics.modalInsets : Metrics.confirmWidth)
    // As tall as what it lists, up to the window: then the list scrolls and
    // Close stays in reach. A fixed height once let the list run over Close.
    height: Math.min(parent ? parent.height - Metrics.modalInsets : body.contentHeight, body.contentHeight + Metrics.gap12 + close.height + Metrics.modalInsets) // derived: the list, the gap and Close inside the frame
    Flickable {
        id: body
        objectName: "pluginRepositoriesBody"
        anchors.top: parent.top
        anchors.bottom: close.top
        anchors.bottomMargin: Metrics.gap12
        width: parent.width
        contentHeight: content.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: P.ViewportScrollBar { gutter: -Metrics.gap8 }
        Column {
            id: content
            width: body.width - Metrics.gap12
            spacing: Metrics.gap8
            // A repository serves either section: native plugins, or a catalog
            // of compatible ones; Chilpa tells them apart.
            Row {
                width: parent.width
                spacing: Metrics.gap8
                P.TextField {
                    id: address
                    objectName: "pluginRepositoryAddress"
                    width: Math.max(0, parent.width - add.width - Metrics.gap8)
                    placeholder: I18n.tr("Plugin repository URL", I18n.rev)
                }
                P.PillButton {
                    id: add
                    objectName: "addPluginRepository"
                    text: I18n.tr("Add", I18n.rev)
                    enabled: !dialog.manager.view.busy && address.text.trim() !== ""
                    onClicked: { dialog.manager.command("add", {url: address.text}); address.text = "" }
                }
            }
            Text { topPadding: Metrics.gap8; text: I18n.tr("Chilpa plugins", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.titleSmall }
            Text { text: dialog.manager.view.native_repository || ""; textFormat: Text.PlainText; width: parent.width; wrapMode: Text.WrapAnywhere; color: Theme.color.textSecondary; font.pointSize: Type.body }
            P.PillButton { text: I18n.tr("Refresh", I18n.rev); enabled: !dialog.manager.view.busy; onClicked: dialog.manager.command("refresh_repository", {id: "native"}) }
            // Offered in every profile, as the one above: it can be read again, not removed.
            Column {
                objectName: "thirdPartyRepository"
                visible: (dialog.manager.view.third_party_repository || "") !== ""
                width: parent.width
                topPadding: Metrics.gap8
                spacing: Metrics.gap8
                Text { text: I18n.tr("Third-party plugins", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.titleSmall }
                Text { text: dialog.manager.view.third_party_repository || ""; textFormat: Text.PlainText; width: parent.width; wrapMode: Text.WrapAnywhere; color: Theme.color.textSecondary; font.pointSize: Type.body }
                P.PillButton { text: I18n.tr("Refresh", I18n.rev); enabled: !dialog.manager.view.busy; onClicked: dialog.manager.command("refresh_repository", {id: "third-party"}) }
            }
            // The repositories the listener added, each to read again or remove.
            Repeater {
                model: dialog.manager.view.repositories || []
                delegate: Column {
                    required property var modelData
                    objectName: "addedPluginRepository"
                    width: content.width
                    topPadding: Metrics.gap8
                    spacing: Metrics.gap8
                    Text { text: modelData.url; textFormat: Text.PlainText; width: parent.width; wrapMode: Text.WrapAnywhere; color: Theme.color.textSecondary; font.pointSize: Type.body }
                    Row {
                        spacing: Metrics.gap8
                        P.PillButton { text: I18n.tr("Refresh", I18n.rev); enabled: !dialog.manager.view.busy; onClicked: dialog.manager.command("refresh_repository", {id: modelData.id}) }
                        P.PillButton { text: I18n.tr("Remove", I18n.rev); enabled: !dialog.manager.view.busy; onClicked: dialog.manager.command("remove_repository", {id: modelData.id}) }
                    }
                }
            }
        }
    }
    P.PillButton { id: close; objectName: "closePluginRepositories"; anchors.bottom: parent.bottom; anchors.right: parent.right; text: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
}
