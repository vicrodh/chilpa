import QtQuick
import app.chilpa
import app.chilpa.metadata
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

Item {
    id: host
    property bool current: false
    function present() {
        if (current && MetadataEditor.discogsQuota && MetadataEditor.takeDiscogsQuota()) dialog.open()
    }
    function settings(key) {
        Visit.keep("settings.focus",{section:"integrations",key:key})
        Navigation.openSection("integrations")
    }
    onCurrentChanged: { if (current) present(); else dialog.close() }
    Component.onCompleted: present()
    Connections { target: MetadataEditor; function onDiscogsQuotaChanged() { host.present() } }
    QuotaDialog {
        id: dialog
        onTokenRequested: host.settings("integrations.discogs.token")
        onOauthRequested: host.settings("integrations.discogs.account")
    }
}
