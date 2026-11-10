import QtQuick
import QtQuick.Dialogs
import app.chilpa.kit
import "../Theme"
import "../Primitives"

Column {
    id: choice
    property string title
    property string description
    property string folder: ""
    property string defaultLabel
    property string defaultFolder: ""
    property string error: ""
    signal chosen(string folder)
    spacing: Metrics.gap6
    onVisibleChanged: if (!visible) picker.close()
    Text {
        width: parent.width
        text: choice.title
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        font.pointSize: Type.body
        font.weight: Type.semibold
        color: Theme.color.textPrimary
    }
    Text {
        width: parent.width
        visible: text !== ""
        text: choice.description
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
    Text {
        objectName: "folderChoicePath"
        width: parent.width
        text: {
            const folder = choice.folder || choice.defaultFolder
            if (!folder) return choice.defaultLabel
            try { return decodeURIComponent(folder) }
            catch (error) { return folder }
        }
        textFormat: Text.PlainText
        elide: Text.ElideMiddle
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
    Flow {
        width: parent.width
        spacing: Metrics.gap8
        PillButton {
            objectName: "chooseFolder"
            text: I18n.tr("Choose a folder…",I18n.rev)
            maximumWidth: choice.width
            wrapText: true
            onClicked: picker.open()
        }
        PillButton {
            objectName: "defaultFolder"
            visible: choice.folder !== ""
            text: choice.defaultLabel
            maximumWidth: choice.width
            wrapText: true
            onClicked: choice.chosen("")
        }
    }
    Text {
        width: parent.width
        visible: text !== ""
        text: choice.error
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: Theme.color.statusNegative
        font.pointSize: Type.small
    }
    FolderDialog {
        id: picker
        title: choice.title
        currentFolder: choice.folder || choice.defaultFolder
        onAccepted: choice.chosen(selectedFolder.toString())
    }
}
