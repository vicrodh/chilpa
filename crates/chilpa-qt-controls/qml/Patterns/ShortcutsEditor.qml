import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.controls
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"
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
    readonly property var keymaps: ["default", "letters"]

    // The room the window leaves it, once it is in one.
    readonly property real room: parent ? parent.height - Metrics.shortcutsAround : 0

    width: Math.min((parent ? parent.width : 0) - Metrics.shortcutsAround, Metrics.shortcutsEditor)
    // While a key is being recorded Escape is that recording's, not the
    // editor's.
    closePolicy: recording ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onAboutToShow: Hotkeys.refresh()
    onClosed: Hotkeys.command("cancel", "")

    Column {
        width: editor.width - Metrics.modalInsets
        spacing: Metrics.gap16

        Item {
            width: parent.width
            implicitHeight: close.height

            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: Metrics.gap10

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: I18n.tr("Customize Shortcuts", I18n.rev)
                    font.pointSize: Type.section
                    font.weight: Type.semibold
                    color: Theme.color.textPrimary
                }
                Chip {
                    objectName: "modified"
                    anchors.verticalCenter: parent.verticalCenter
                    visible: (editor.keys.modified || 0) > 0
                    text: I18n.trArgs("{} modified", [String(editor.keys.modified || 0)], I18n.rev)
                }
            }
            Row {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
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
                    options: [{ text: I18n.tr("Default", I18n.rev) }, { text: I18n.tr("Letters", I18n.rev) }]
                    current: Math.max(0, editor.keymaps.indexOf(editor.keys.keymap))
                    onActivated: (index) => Hotkeys.command("keymap", editor.keymaps[index])
                }
                IconButton {
                    id: close
                    tip: I18n.tr("Close", I18n.rev)
                    glyph: Icons.x
                    onClicked: editor.close()
                }
            }
        }
        ShortcutColumns {
            width: parent.width
            height: Math.min(implicitHeight, Math.max(0, editor.room - Sizes.sheetChrome))
            keys: editor.keys
            editing: true
        }
        PillButton {
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
