import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

Modal {
    id: dialog
    objectName: "playlistFolderDialog"
    property int folderId: 0
    property string colorName: "accent"
    property string notice: ""
    width: Math.max(Metrics.modalInsets, Math.min(Metrics.playlistFolderDialog, parent.width - Metrics.modalInset))

    function show(id, name, color) {
        folderId = id
        nameField.text = name
        colorName = color
        notice = ""
        open()
        Qt.callLater(nameField.focusInput)
    }
    function save() {
        if (nameField.text.trim() === "") return
        const saved = folderId > 0 ? Playlists.updateFolder(folderId, nameField.text, colorName)
                                  : Playlists.createFolder(nameField.text, colorName) > 0
        if (saved) close()
        else notice = I18n.tr("Could not save the folder. Please try again.", I18n.rev)
    }

    Column {
        width: parent.width
        spacing: Metrics.gap12
        Row {
            width: parent.width
            Text {
                width: parent.width - closeButton.width
                text: dialog.folderId > 0 ? I18n.tr("Edit folder", I18n.rev) : I18n.tr("New folder", I18n.rev)
                font.pointSize: Type.titleSmall
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            IconButton { id: closeButton; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
        }
        TextField {
            id: nameField
            objectName: "folderName"
            width: parent.width
            placeholder: I18n.tr("Folder name", I18n.rev)
            onAccepted: dialog.save()
        }
        Row {
            spacing: Metrics.gap8
            Repeater {
                model: ["accent", "positive", "warning", "negative"]
                Rectangle {
                    required property string modelData
                    width: Metrics.colorSwatch
                    height: width
                    radius: Metrics.colorSwatchRadius
                    color: modelData === "positive" ? Theme.color.statusPositive : modelData === "warning" ? Theme.color.statusWarning
                         : modelData === "negative" ? Theme.color.statusNegative : Theme.color.accent
                    border.width: dialog.colorName === modelData ? Metrics.focusRing : 0
                    border.color: Theme.color.textPrimary
                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: dialog.colorName = parent.modelData }
                }
            }
        }
        Text {
            visible: dialog.notice !== ""
            width: parent.width
            text: dialog.notice
            wrapMode: Text.Wrap
            font.pointSize: Type.small
            color: Theme.color.statusWarning
        }
        Row {
            anchors.right: parent.right
            spacing: Metrics.gap8
            PillButton { text: I18n.tr("Cancel", I18n.rev); onClicked: dialog.close() }
            PillButton {
                objectName: "saveFolder"
                kind: "primary"
                text: dialog.folderId > 0 ? I18n.tr("Save", I18n.rev) : I18n.tr("Create", I18n.rev)
                enabled: nameField.text.trim() !== ""
                onClicked: dialog.save()
            }
        }
    }
}
