import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Modal {
    id: dialog
    objectName: "discogsQuotaDialog"
    signal tokenRequested()
    signal oauthRequested()
    width: Metrics.confirmWidth
    Column {
        width: parent.width; spacing: Metrics.gap12
        Text {
            width: parent.width; wrapMode: Text.WordWrap; textFormat: Text.PlainText
            text: I18n.tr("Discogs is out of requests", I18n.rev)
            font.pointSize: Type.titleSmall; font.weight: Type.bold; color: Theme.color.textPrimary
        }
        Text {
            width: parent.width; wrapMode: Text.WordWrap; textFormat: Text.PlainText
            text: I18n.tr("Discogs limits how many requests one connection can make. To keep searching, connect your account with a personal token or by signing in.", I18n.rev)
            font.pointSize: Type.body; color: Theme.color.textSecondary
        }
        Flow {
            width: parent.width; spacing: Metrics.gap8
            PillButton { objectName: "discogsUseToken"; text: I18n.tr("Use a token", I18n.rev); onClicked: { dialog.close(); dialog.tokenRequested() } }
            PillButton { objectName: "discogsSignIn"; kind: "primary"; text: I18n.tr("Sign in with Discogs", I18n.rev); onClicked: { dialog.close(); dialog.oauthRequested() } }
            PillButton { objectName: "discogsNotNow"; text: I18n.tr("Not now", I18n.rev); onClicked: dialog.close() }
        }
    }
}
