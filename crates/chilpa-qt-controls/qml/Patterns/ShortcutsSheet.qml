import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"
import "../Components"

// Every keyboard shortcut at a glance. It is read here; changing one is the
// editor's, which its foot opens.
Modal {
    id: sheet
    objectName: "shortcutsSheet"

    readonly property var keys: JSON.parse(Hotkeys.snapshot || "{}")

    signal customize()

    // The room the window leaves it, once it is in one.
    readonly property real room: parent ? parent.height - Metrics.shortcutsAround : 0

    width: Math.min((parent ? parent.width : 0) - Metrics.shortcutsAround, Metrics.shortcutsSheet)
    onAboutToShow: Hotkeys.refresh()

    Column {
        width: sheet.width - Metrics.modalInsets
        spacing: Metrics.gap16

        Item {
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
        ShortcutColumns {
            width: parent.width
            height: Math.min(implicitHeight, Math.max(0, sheet.room - Sizes.sheetChrome))
            keys: sheet.keys
        }
        PillButton {
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
