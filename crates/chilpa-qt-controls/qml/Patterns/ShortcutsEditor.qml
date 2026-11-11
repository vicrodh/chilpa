import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"
import "../Components"

// Where a shortcut is given another key: press its key cap, then the keys.
// A key another action has is refused and said; a free one is taken at
// once. The keymap picks which set of keys everything starts from.
Modal {
    id: editor
    objectName: "shortcutsEditor"

    readonly property var keys: JSON.parse(Hotkeys.snapshot || "{}")
    readonly property bool recording: (keys.recording || "") !== ""
    readonly property var keymaps: ["default", "letters", "vi"]

    preferredWidth: Math.min((parent ? parent.width : 0) - Metrics.shortcutsAround, Metrics.shortcutsEditor)
    // While a key is being recorded Escape is that recording's, not the
    // editor's.
    closePolicy: recording ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnReleaseOutside
    onAboutToShow: Hotkeys.refresh()
    onClosed: Hotkeys.command("cancel", "")
    onRecordingChanged: if (recording) contentItem.forceActiveFocus()
    Connections {
        target: editor.contentItem.Keys
        function onPressed(event) {
            if (!editor.recording) return
            Hotkeys.pressedIn("window", event.key, event.modifiers, event.text, false)
            event.accepted = true
        }
    }

    Column {
        width: editor.width - Metrics.modalInsets
        spacing: Metrics.gap16

        // The title and its count, the keymap beside them while all of it
        // fits one line and under them when it does not, the close button in
        // the corner.
        Item {
            id: head
            width: parent.width
            readonly property real line: Math.max(title.height, close.height)
            readonly property bool stacked: title.width + keymap.width + close.width + keymap.spacing * 2 > width // derived: a gap on each side of the keymap
            implicitHeight: stacked ? line + keymap.spacing + keymap.height : Math.max(line, keymap.height)

            Row {
                id: title
                y: (head.line - height) / 2 // derived: centred on the first line
                spacing: Metrics.gap10

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, head.width - close.width - title.spacing - (count.visible ? count.width + title.spacing : 0))
                    text: I18n.tr("Customize Shortcuts", I18n.rev)
                    wrapMode: Text.Wrap
                    font.pointSize: Type.section
                    font.weight: Type.semibold
                    color: Theme.color.textPrimary
                }
                Chip {
                    id: count
                    objectName: "modified"
                    anchors.verticalCenter: parent.verticalCenter
                    visible: (editor.keys.modified || 0) > 0
                    text: I18n.trArgs("{} modified", [String(editor.keys.modified || 0)], I18n.rev)
                }
            }
            Row {
                id: keymap
                x: head.stacked ? 0 : close.x - spacing - width
                y: head.stacked ? head.line + spacing : (head.line - height) / 2 // derived: under the title, or centred beside it
                spacing: Metrics.gap10

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: I18n.tr("Keymap", I18n.rev)
                    font.pointSize: Type.meta
                    color: Theme.color.textTertiary
                }
                SegmentedControl {
                    objectName: "keymap"
                    anchors.verticalCenter: parent.verticalCenter
                    options: [{ text: I18n.tr("Default", I18n.rev) }, { text: I18n.tr("Letters", I18n.rev) }, { text: "Vi" }]
                    current: Math.max(0, editor.keymaps.indexOf(editor.keys.keymap))
                    onActivated: (index) => Hotkeys.command("keymap", editor.keymaps[index])
                }
            }
            IconButton {
                id: close
                anchors.right: parent.right
                y: (head.line - height) / 2 // derived: centred on the first line
                tip: I18n.tr("Close", I18n.rev)
                glyph: Icons.x
                onClicked: editor.close()
            }
        }
        // What the modal's height leaves once its head and foot are in.
        ShortcutColumns {
            width: parent.width
            height: Math.min(implicitHeight, Math.max(0, editor.maximumHeight - Metrics.modalInsets - head.height - foot.height - parent.spacing * 2)) // derived: the gaps above and below
            keys: editor.keys
            editing: true
        }
        PillButton {
            id: foot
            objectName: "resetAll"
            anchors.horizontalCenter: parent.horizontalCenter
            enabled: (editor.keys.modified || 0) > 0
            opacity: enabled ? 1 : Sizes.idle
            glyph: Icons.refresh
            text: I18n.tr("Reset All to Defaults", I18n.rev)
            onClicked: Hotkeys.command("reset-all", "")
        }
    }

}
