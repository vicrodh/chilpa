import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Modal {
    id: dialog
    property int serverId: -1
    property var preview: null
    readonly property var setup: preview || JSON.parse(Servers.setup || "{}")
    readonly property string step: setup.step || "idle"
    function show(id) { serverId = id; open() }

    width: Math.min(Metrics.confirmWidth + Metrics.modalInsets, parent.width - Metrics.modalInsets)
    height: Math.min(body.naturalHeight + Metrics.modalInsets, parent.height - Metrics.modalInsets)
    onAboutToShow: {
        if (!preview) {
            if (serverId < 0) Servers.cancel()
            else Servers.begin("edit/" + serverId)
        }
    }
    onClosed: if (!preview) Servers.cancel()

    Column {
        id: body
        width: parent.width
        height: parent.height
        readonly property real naturalHeight: title.implicitHeight + help.implicitHeight + editor.implicitHeight + spacing * 2 // derived: two gaps between three sections
        spacing: Metrics.gap16
        Text {
            id: title
            width: parent.width
            text: dialog.serverId >= 0 ? I18n.tr("Edit server", I18n.rev)
                : dialog.step === "done" ? I18n.tr("Server added", I18n.rev) : I18n.tr("Add a server", I18n.rev)
            font.pointSize: Type.section
            font.weight: Type.semibold
            color: Theme.color.textPrimary
        }
        Text {
            id: help
            width: parent.width
            text: dialog.step === "code" && (dialog.setup.link || "") !== "" ? I18n.tr("Type this code at plex.tv/link, signed in to your Plex account.", I18n.rev)
                : dialog.step === "code" ? I18n.tr("Type this code in Quick Connect, in a Jellyfin you are signed in to.", I18n.rev)
                : dialog.step === "servers" ? I18n.tr("Your account reaches more than one server. Which one?", I18n.rev)
                : dialog.step === "libraries" ? I18n.tr("Which of its libraries come in?", I18n.rev)
                : dialog.step === "done" ? I18n.tr("Its music is being read; it appears in the library as it arrives.", I18n.rev)
                : dialog.serverId >= 0 ? I18n.tr("Update the connection, sign-in or libraries for this server.", I18n.rev)
                : I18n.tr("Its music joins your library, listed with your own files.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Flickable {
            objectName: "serverFormViewport"
            width: parent.width
            implicitHeight: editor.implicitHeight
            height: Math.max(0, body.height - y)
            contentHeight: editor.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap4 }
            Loader {
                id: editor
                width: parent.width
                active: dialog.visible
                sourceComponent: ServerEditForm {
                    width: editor.width
                    editing: dialog.serverId >= 0
                    preview: dialog.preview
                    onClosed: dialog.close()
                }
            }
        }
    }
}
