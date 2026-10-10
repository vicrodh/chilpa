import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.link
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// QBZ's "Open Music Link" (LinkResolverModal.qml): a field for the link, Go,
// what went wrong, and for a playlist of another service the way to the
// importer. The facade resolves the link off the window's thread; this is
// the surface, opened by the app menu or by its key.
Modal {
    id: modal
    width: Math.min((parent ? parent.width : Metrics.linkModal) - Metrics.modalInsets, Metrics.linkModal)

    onClosed: if (Link.open) Link.close()

    // The menu and the key ask for it alike.
    Connections {
        target: Hotkeys
        function onLinkAsked() { Link.show() }
    }
    Connections {
        target: Link
        function onOpenChanged() {
            if (!Link.open)
                return modal.close()
            field.text = ""
            modal.open()
            field.focusInput()
        }
        function onOpened(screen, id, key) {
            if (key !== "")
                Navigation.goKey(screen, key)
            else
                Navigation.go(screen, id)
        }
    }

    Column {
        width: modal.width - Metrics.modalInsets
        spacing: Metrics.gap16

        Item {
            width: parent.width
            height: close.height
            Text {
                anchors.left: parent.left
                anchors.right: close.left
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Open music link", I18n.rev)
                font.pointSize: Type.titleSmall
                font.weight: Type.bold
                color: Theme.color.textPrimary
                elide: Text.ElideRight
            }
            IconButton {
                id: close
                anchors.right: parent.right
                tip: I18n.tr("Close", I18n.rev)
                glyph: Icons.x
                onClicked: Link.close()
            }
        }

        Row {
            width: parent.width
            spacing: Metrics.gap8
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                glyph: Lucide.link
                iconFamily: Lucide.font
                size: Metrics.iconMd
                color: Theme.color.textTertiary
            }
            TextField {
                id: field
                objectName: "linkField"
                width: parent.width - Metrics.iconMd - go.width - Metrics.gap8 * 2 // derived: the glyph, the button and the gaps
                placeholder: I18n.tr("Paste a link from one of your sources or from another service", I18n.rev)
                enabled: !Link.resolving
                onAccepted: if (text.trim() !== "") Link.submit(text)
            }
            PillButton {
                id: go
                objectName: "linkGo"
                anchors.verticalCenter: parent.verticalCenter
                kind: "primary"
                text: Link.resolving ? I18n.tr("Resolving…", I18n.rev) : I18n.tr("Go", I18n.rev)
                enabled: field.text.trim() !== "" && !Link.resolving
                onClicked: Link.submit(field.text)
            }
        }

        Text {
            objectName: "linkError"
            width: parent.width
            visible: Link.error !== ""
            text: Link.error
            wrapMode: Text.WordWrap
            font.pointSize: Type.small
            color: Theme.color.statusNegative
        }

        Column {
            objectName: "linkPlaylist"
            width: parent.width
            visible: Link.playlist
            spacing: Metrics.gap8
            Text {
                width: parent.width
                text: I18n.tr("This looks like a playlist.", I18n.rev)
                wrapMode: Text.WordWrap
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
            PillButton {
                objectName: "linkImporter"
                text: I18n.tr("Open playlist importer", I18n.rev)
                onClicked: if (Link.openImporter()) Navigation.go("playlistImport", 0)
            }
        }
    }
}
