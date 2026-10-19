import QtQuick
import app.chilpa.themes
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Patterns"

// The theme's picker and the options under it, as Settings shows them, and
// the custom theme's editor with a swatch open on its picker.
Item {
    id: gallery
    implicitWidth: column.implicitWidth + Metrics.contentX * 2 // derived: the sample and its margins
    implicitHeight: column.implicitHeight + Metrics.contentX * 2 // derived: the sample and its margins
    readonly property int sampleWide: 640

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentX
        spacing: Metrics.contentX
        ThemePicker { options: ["Dark", "Nord", "Alucard", "Auto (dynamic)", "Custom"]; values: ["dark", "nord", "alucard", "auto", "custom"]; value: "nord" }
        ThemeOptionsPanel { width: gallery.sampleWide }
        OptionsBox {
            width: gallery.sampleWide
            title: "Custom"
            ThemeOptionRow { width: parent.width; title: "Dark theme"; description: "Set the overall light or dark polarity."; Toggle { checked: true } }
            CustomThemeEditor {
                width: parent.width
                openKey: "accent"
                colors: ({ "surface-main": Theme.color.surfaceWindow, "surface-card": Theme.color.surfacePanel, "surface-elevated": Theme.color.surfaceRaised,
                    "text-primary": Theme.color.textPrimary, "text-secondary": Theme.color.textSecondary, "accent": Theme.color.accent,
                    "danger": Theme.color.statusNegative, "warning": Theme.color.statusWarning, "success": Theme.color.statusPositive,
                    "border": Theme.color.strokeHairline, "favorite": Theme.color.favorite })
            }
        }
        Row {
            spacing: Metrics.gap10
            ColorCell { label: "Accent"; swatch: Theme.color.accent; open: true }
            ColorCell { label: "Border"; swatch: Theme.color.strokeHairline }
        }
        ColorPicker { width: gallery.sampleWide; value: Theme.color.accent }
    }
}
