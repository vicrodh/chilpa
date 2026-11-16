import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// Every keyboard shortcut at a glance. It is read here; changing one is the
// editor's, which its foot opens.
Modal {
    id: sheet
    objectName: "shortcutsSheet"

    readonly property var keys: JSON.parse(Hotkeys.snapshot || "{}")

    signal customize()

    preferredWidth: Math.min((parent ? parent.width : 0) - Metrics.shortcutsAround, Metrics.shortcutsSheet)
    onAboutToShow: Hotkeys.refresh()

    Column {
        width: sheet.width - Metrics.modalInsets
        spacing: Metrics.gap16

        Item {
            id: head
            width: parent.width
            implicitHeight: close.height

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Keyboard Shortcuts", I18n.rev)
                font.pointSize: Type.section
                font.weight: Type.semibold
                color: Theme.color.textPrimary
            }
            IconButton {
                id: close
                anchors.right: parent.right
                tip: I18n.tr("Close", I18n.rev)
                glyph: Icons.x
                onClicked: sheet.close()
            }
        }
        // What the modal's height leaves once its head and foot are in.
        ShortcutColumns {
            width: parent.width
            height: Math.min(implicitHeight, Math.max(0, sheet.maximumHeight - Metrics.modalInsets - head.height - foot.height - parent.spacing * 2)) // derived: the gaps above and below
            keys: sheet.keys
        }
        PillButton {
            id: foot
            objectName: "customize"
            anchors.horizontalCenter: parent.horizontalCenter
            text: I18n.tr("Customize Shortcuts", I18n.rev)
            onClicked: {
                sheet.close()
                sheet.customize()
            }
        }
    }
}
