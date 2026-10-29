import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.link
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The facade verifies destinations before this surface offers a choice.
Modal {
    id: modal
    width: Math.min((parent ? parent.width : Metrics.linkModal) - Metrics.modalInsets, Metrics.linkModal + Metrics.sidebarWidth)
    readonly property var destinations: {
        try { return JSON.parse(Link.choices || "[]") } catch (_) { return [] }
    }
    onDestinationsChanged: if (destinations.length > 0) Qt.callLater(focusChoice, 0)
    function focusChoice(index) {
        const item = choiceRepeater.itemAt(index)
        if (visible && item) item.forceActiveFocus(Qt.TabFocusReason)
    }

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

        Text {
            width: parent.width
            text: I18n.tr("Find this music in your library and connected sources.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
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
                text: Link.resolving ? I18n.tr("Searching…", I18n.rev) : I18n.tr("Find music", I18n.rev)
                enabled: field.text.trim() !== "" && !Link.resolving
                onClicked: Link.submit(field.text)
            }
        }

        Row {
            objectName: "linkProgress"
            width: parent.width
            visible: Link.resolving
            spacing: Metrics.gap8
            BusyMark {
                clock: Pulse
                active: modal.visible && Link.resolving
                anchors.verticalCenter: parent.verticalCenter
                size: Metrics.iconMd
                color: Theme.color.accent
            }
            Text {
                width: parent.width - Metrics.iconMd - Metrics.gap8 // derived: remaining space after the status icon
                text: I18n.tr("Checking your library and connected sources…", I18n.rev)
                wrapMode: Text.WordWrap
                font.pointSize: Type.small
                color: Theme.color.textSecondary
                Accessible.role: Accessible.StaticText
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
            objectName: "linkChoices"
            width: parent.width
            spacing: Metrics.gap8
            visible: modal.destinations.length > 0
            Text {
                text: I18n.tr("Choose where to open this music", I18n.rev)
                font.pointSize: Type.body
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            Flickable {
                id: choicesScroll
                width: parent.width
                height: Math.min(choiceList.implicitHeight, modal.parent ? modal.parent.height / 2 : Metrics.settingsRow * 3) // derived: choices use at most half the window
                contentHeight: choiceList.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                Column {
                    id: choiceList
                    width: parent.width
                    spacing: Metrics.gap8
                    Repeater {
                        id: choiceRepeater
                        model: modal.destinations
                        LinkDestination {
                            required property var modelData
                            required property int index
                            width: parent.width
                            destination: modelData
                            onChosen: Link.choose(index)
                            onMoveFocus: delta => modal.focusChoice(Math.max(0, Math.min(modal.destinations.length - 1, index + delta)))
                            onActiveFocusChanged: if (activeFocus)
                                choicesScroll.contentY = Math.max(0, Math.min(y, choiceList.implicitHeight - choicesScroll.height))
                        }
                    }
                }
            }
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
