import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Responsive actions for plugin inspection, permissions and account settings.
Item {
    id: footer
    property var plugin: ({})
    property var view: ({})
    property string mode
    signal uninstallRequested()
    signal refreshRequested()
    signal permissionAccepted()
    signal closeRequested()
    implicitHeight: Math.max(actions.implicitHeight, close.implicitHeight)
    Flow {
        id: actions
        width: Math.max(0, parent.width - close.width - Metrics.gap12)
        spacing: Metrics.gap8
        PillButton {
            objectName: "uninstallPlugin"
            maximumWidth: actions.width
            wrapText: true
            visible: footer.mode === "settings"
            enabled: !footer.view.busy
            text: I18n.tr("Uninstall plugin and remove account data", I18n.rev)
            onClicked: footer.uninstallRequested()
        }
        PillButton {
            objectName: "refreshPluginSettings"
            maximumWidth: actions.width
            wrapText: true
            visible: footer.mode === "settings"
            text: I18n.tr("Refresh settings", I18n.rev)
            glyph: Lucide.refreshCcwDot
            iconFamily: Lucide.font
            enabled: !footer.view.busy
            onClicked: footer.refreshRequested()
        }
        PillButton {
            objectName: "acceptPluginPermissions"
            maximumWidth: actions.width
            wrapText: true
            visible: footer.mode === "permissions"
            text: I18n.tr("Accept permissions and enable", I18n.rev)
            enabled: !footer.view.busy && !!footer.plugin.revision
            onClicked: footer.permissionAccepted()
        }
    }
    PillButton {
        id: close
        objectName: "closePluginDetails"
        anchors.right: parent.right
        maximumWidth: parent.width
        wrapText: true
        text: I18n.tr("Close", I18n.rev)
        onClicked: footer.closeRequested()
    }
}
