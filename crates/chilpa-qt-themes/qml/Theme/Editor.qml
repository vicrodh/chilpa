pragma Singleton
import QtQuick

// The measures of the theme's options and the custom theme's editor, QBZ
// 2.1.2's (`CustomThemeEditor.qml`, `QbzColorPicker.qml`,
// `SettingsFieldset.qml`), that the window's theme does not already name.
QtObject {
    // A base colour's swatch, and the room between two groups of them.
    readonly property int swatchLong: 56
    readonly property int swatchTall: 32
    readonly property int groupsApart: 28
    // The picker: its saturation and value square, the hue strip, the
    // crosshair and the strip's marker, and the preview beside the HEX.
    readonly property int shadeField: 140
    readonly property int hueBand: 14
    readonly property int crosshair: 14
    readonly property int hueMarker: 4
    readonly property int previewLong: 40
    readonly property int previewTall: 34
    readonly property int hexLeast: 120
    // The header of a box of options.
    readonly property int boxHeader: 40
}
