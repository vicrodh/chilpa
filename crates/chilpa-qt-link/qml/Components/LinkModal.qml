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
    width: Math.min((parent ? parent.width : Metrics.linkModal) - Metrics.modalInsets, Metrics.linkModal + Metrics.sidebarWidth)
    readonly property var destinations: {
        try { return JSON.parse(Link.choices || "[]") } catch (_) { return [] }
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
            objectName: "linkChoices"
            width: parent.width
            spacing: Metrics.gap8
            visible: modal.destinations.length > 0
            Text {
                text: I18n.tr("Open in", I18n.rev)
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
            Flickable {
                id: choicesScroll
                width: parent.width
                height: Math.min(choiceList.implicitHeight, Metrics.settingsRow * 5 + Metrics.gap8 * 4) // derived: five visible choices
                contentHeight: choiceList.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                Column {
                    id: choiceList
                    width: parent.width
                    spacing: Metrics.gap8
                    Repeater {
                        model: modal.destinations
                        Rectangle {
                            required property var modelData
                            required property int index
                            objectName: "linkChoice"
                            width: parent.width
                            height: Metrics.settingsRow
                            radius: Metrics.radiusControl
                            color: chooseMouse.containsMouse ? Theme.color.stateHover : Theme.color.surfacePanel
                            activeFocusOnTab: true
                            border.width: activeFocus ? Metrics.focusRing : 0
                            border.color: Theme.color.accent
                            onActiveFocusChanged: if (activeFocus)
                                choicesScroll.contentY = Math.max(0, Math.min(y, choiceList.implicitHeight - choicesScroll.height))
                            Keys.onPressed: (event) => {
                                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                                    Link.choose(index)
                                    event.accepted = true
                                }
                            }
                            Row {
                                anchors.fill: parent
                                anchors.margins: Metrics.gap12
                                spacing: Metrics.gap12
                                Text {
                                    width: Metrics.sidebarWidth / 2 // derived: compact source label
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: modelData.source || ""
                                    color: Theme.color.accent
                                    font.pointSize: Type.small
                                    elide: Text.ElideRight
                                }
                                Text {
                                    width: parent.width - Metrics.sidebarWidth / 2 - Metrics.gap12 // derived: remaining title space
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: modelData.title + (modelData.artist ? " · " + modelData.artist : "")
                                    color: Theme.color.textPrimary
                                    font.pointSize: Type.small
                                    elide: Text.ElideRight
                                }
                            }
                            MouseArea {
                                id: chooseMouse
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: Link.choose(index)
                            }
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
