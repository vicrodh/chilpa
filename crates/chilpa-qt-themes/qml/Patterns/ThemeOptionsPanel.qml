import QtQuick
import QtQuick.Dialogs
import app.chilpa.kit
import app.chilpa.themes
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The options of the theme chosen, in a box under its picker (QBZ 2.1.2,
// `AppearanceSettings.qml`). The auto theme's: where its colours come
// from, the image when that is the source, the desktop it reads and a way
// to generate it again. The custom theme's: starting from the colours on
// screen, its polarity, and its eleven base colours with their picker.
// There is no save, no list of custom themes and no import: one custom
// theme, written as it is edited, as in QBZ.
Item {
    id: panel

    readonly property var doc: {
        try { return JSON.parse(ThemeEditor.state) } catch (_) { return ({}) }
    }
    readonly property var auto: panel.doc.auto || ({})
    readonly property var custom: panel.doc.custom || ({})
    readonly property var colors: panel.custom.state ? panel.custom.state.colors : ({})
    readonly property bool imageSource: panel.auto.source === 2 // derived: the third source, an image

    implicitHeight: panel.doc.theme === "auto" ? autoBox.implicitHeight
        : panel.doc.theme === "custom" ? customBox.implicitHeight : 0

    OptionsBox {
        id: autoBox
        objectName: "autoOptions"
        width: panel.width
        visible: panel.doc.theme === "auto"
        title: panel.doc.title || ""
        folded: panel.auto.folded === true
        onFoldAsked: ThemeEditor.fold("auto", !folded)

        ThemeOptionRow {
            width: parent.width
            title: I18n.tr("Source", I18n.rev)
            description: I18n.tr("Generate a color theme from your system wallpaper or a custom image", I18n.rev)
            SelectField {
                objectName: "autoSource"
                options: panel.auto.sources || []
                values: (panel.auto.sources || []).map((_, i) => String(i))
                value: String(panel.auto.source || 0)
                onPicked: (value) => ThemeEditor.setSource(Number(value))
            }
        }
        // The image in use is the row's name; with none, it repeats the button.
        ThemeOptionRow {
            width: parent.width
            visible: panel.imageSource
            title: panel.auto.image ? panel.auto.image : I18n.tr("Select Image...", I18n.rev)
            PillButton {
                objectName: "autoImage"
                text: I18n.tr("Select Image...", I18n.rev)
                onClicked: imageDialog.open()
            }
        }
        ThemeOptionRow {
            width: parent.width
            visible: (panel.auto.desktop || "") !== ""
            title: I18n.tr("Detected: ", I18n.rev) + (panel.auto.desktop || "")
            description: I18n.tr("Experimental: theme may not match your system exactly.", I18n.rev)
        }
        ThemeOptionRow {
            width: parent.width
            title: I18n.tr("Regenerate", I18n.rev)
            PillButton {
                objectName: "autoRegenerate"
                text: I18n.tr("Regenerate", I18n.rev)
                onClicked: ThemeEditor.regenerate()
            }
        }
    }

    OptionsBox {
        id: customBox
        objectName: "customOptions"
        width: panel.width
        visible: panel.doc.theme === "custom"
        title: panel.doc.title || ""
        folded: panel.custom.folded === true
        onFoldAsked: {
            // A picker left open in a folded box would come back open.
            editor.closePicker()
            ThemeEditor.fold("custom", !folded)
        }

        ThemeOptionRow {
            width: parent.width
            title: I18n.tr("Start from current theme", I18n.rev)
            description: I18n.tr("Copy the colors of the currently applied theme into the editor as a starting point.", I18n.rev)
            PillButton {
                objectName: "customSeed"
                text: I18n.tr("Use current colors", I18n.rev)
                onClicked: {
                    ThemeEditor.useCurrentColors()
                    editor.closePicker()
                }
            }
        }
        ThemeOptionRow {
            width: parent.width
            title: I18n.tr("Dark theme", I18n.rev)
            description: I18n.tr("Set the overall light or dark polarity. Affects derived shades, borders and overlays.", I18n.rev)
            Toggle {
                objectName: "customDark"
                checked: panel.custom.state ? panel.custom.state.dark === true : true
                onToggled: (on) => ThemeEditor.setDark(on)
            }
        }
        CustomThemeEditor {
            id: editor
            width: parent.width
            visible: panel.doc.theme === "custom"
            colors: panel.colors
            onColorPicked: (key, hex) => ThemeEditor.setColor(key, hex)
            onReleased: ThemeEditor.flush()
            onCommitted: (key, hex) => {
                ThemeEditor.setColor(key, hex)
                ThemeEditor.flush()
            }
        }
    }

    FileDialog {
        id: imageDialog
        title: I18n.tr("Select Image...", I18n.rev)
        nameFilters: [I18n.tr("Image", I18n.rev) + " (*.png *.jpg *.jpeg *.webp *.bmp)"]
        onAccepted: ThemeEditor.setImage(decodeURIComponent(selectedFile.toString().replace(/^file:\/\//, "")))
    }
}
