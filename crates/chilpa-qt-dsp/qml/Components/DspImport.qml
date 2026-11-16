import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: editor
    objectName: "dspImportEditor"
    property alias nameText: profileName.text
    property alias profileText: profile.text
    signal nameEdited(string value)
    signal profileEdited(string value)
    signal submitted(string name, string text)
    signal exported()
    signal libraryRequested(string name, string text)
    spacing: Metrics.gap8
    Text {
        width: parent.width
        text: I18n.tr("Paste AutoEq text, Chilpa JSON or a preset bundle. Poweramp parametric JSON transfers tone, peak and shelf bands. Unsupported settings reject the whole import.", I18n.rev)
        wrapMode: Text.WordWrap
        color: Theme.color.textSecondary
        font.pointSize: Type.body
    }
    TextField {
        id: profileName
        width: parent.width
        placeholder: I18n.tr("Profile name", I18n.rev)
        onTextChanged: if (editing) editor.nameEdited(text)
    }
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
            onTextChanged: if (activeFocus) editor.profileEdited(text)
            color: Theme.color.textPrimary
            selectionColor: Theme.color.rowSelected
            font.pointSize: Type.body
            background: Rectangle { color: Theme.color.fillField; radius: Metrics.radiusControl }
        }
    }
    Flow {
        width: parent.width
        spacing: Metrics.gap8
        PillButton { text: I18n.tr("Apply import", I18n.rev); enabled: profile.text.trim() !== ""; onClicked: editor.submitted(profileName.text.trim() || I18n.tr("Imported", I18n.rev), profile.text) }
        PillButton { text: I18n.tr("Import to library", I18n.rev); enabled: profile.text.trim() !== ""; onClicked: editor.libraryRequested(profileName.text.trim() || I18n.tr("Imported", I18n.rev), profile.text) }
        PillButton { text: I18n.tr("Copy preset JSON", I18n.rev); onClicked: editor.exported() }
    }
}
