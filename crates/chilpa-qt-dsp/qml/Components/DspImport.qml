import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: editor
    signal submitted(string name, string text)
    signal exported()
    spacing: Metrics.gap8
    Text {
        width: parent.width
        text: I18n.tr("Paste a parametric EQ profile or a Chilpa preset JSON. Active unsupported filters reject the whole import.", I18n.rev)
        wrapMode: Text.WordWrap
        color: Theme.color.textSecondary
        font.pointSize: Type.body
    }
    TextField { id: profileName; width: parent.width; placeholder: I18n.tr("Profile name", I18n.rev) }
    Controls.ScrollView {
        width: parent.width
        height: Metrics.dspImportHeight
        Controls.TextArea {
            id: profile
            objectName: "dspProfileText"
            textFormat: TextEdit.PlainText
            selectByMouse: true
            activeFocusOnTab: true
            placeholderText: I18n.tr("Paste EQ filter settings here", I18n.rev)
            color: Theme.color.textPrimary
            selectionColor: Theme.color.rowSelected
            font.pointSize: Type.body
            background: Rectangle { color: Theme.color.fillField; radius: Metrics.radiusControl }
        }
    }
    Row {
        spacing: Metrics.gap8
        PillButton { text: I18n.tr("Apply import", I18n.rev); enabled: profile.text.trim() !== ""; onClicked: editor.submitted(profileName.text.trim() || I18n.tr("Imported", I18n.rev), profile.text) }
        PillButton { text: I18n.tr("Copy preset JSON", I18n.rev); onClicked: editor.exported() }
    }
}
