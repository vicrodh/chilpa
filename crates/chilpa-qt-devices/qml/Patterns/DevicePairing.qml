import QtQuick
import QtQuick.Controls.Basic as Controls
import QtQuick.Dialogs
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Flickable {
    id: form
    property bool mounted: false
    property bool busy: false
    property string chosenFolder: ""
    signal createAsked(string name, string folder)
    signal dismissed()
    contentWidth: width
    contentHeight: fields.height + Metrics.gap16
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    Controls.ScrollBar.vertical: ViewportScrollBar {}

    Column {
        id: fields
        width: Math.min(DeviceLayout.selectWidth + Metrics.contentX, parent.width - Metrics.gap16)
        spacing: Metrics.gap12
        Text { width: parent.width; text: I18n.tr("Add a storage profile", I18n.rev); wrapMode: Text.WordWrap; font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary }
        Text {
            width: parent.width
            text: I18n.tr("Connect a player or memory card. Save a profile for each storage.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Column {
            width: parent.width
            spacing: Metrics.gap4
            Text { text: I18n.tr("Profile name", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
            TextField { id: name; objectName: "deviceProfileName"; width: parent.width; placeholder: I18n.tr("A name for this memory", I18n.rev); enabled: !form.busy }
        }
        Column {
            visible: !form.mounted
            width: parent.width
            spacing: Metrics.gap4
            Text { text: I18n.tr("Music folder", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
            TextField { id: folder; objectName: "deviceManagedFolder"; width: parent.width; text: "Chilpa"; enabled: !form.busy }
        }
        Column {
            visible: form.mounted
            width: parent.width
            spacing: Metrics.gap4
            Text { text: I18n.tr("Folder on the memory card", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
            PillButton { objectName: "deviceChooseFolder"; text: I18n.tr("Choose a folder…", I18n.rev); enabled: !form.busy; onClicked: picker.open() }
            Text {
                width: parent.width
                text: decodeURIComponent(form.chosenFolder.replace(/^file:\/\//, ""))
                textFormat: Text.PlainText
                elide: Text.ElideMiddle
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
        }
        Flow {
            width: parent.width
            spacing: Metrics.gap8
            PillButton {
                objectName: "deviceCreateProfile"
                kind: "primary"
                text: I18n.tr("Save profile", I18n.rev)
                enabled: !form.busy && name.text.trim() !== "" && (form.mounted ? form.chosenFolder !== "" : folder.text.trim() !== "")
                onClicked: form.createAsked(name.text, form.mounted ? form.chosenFolder : folder.text)
            }
            PillButton { text: I18n.tr("Cancel", I18n.rev); enabled: !form.busy; onClicked: form.dismissed() }
        }
    }
    FolderDialog { id: picker; title: I18n.tr("Choose a folder on the memory card", I18n.rev); onAccepted: form.chosenFolder = selectedFolder.toString() }
}
