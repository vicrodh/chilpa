import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The custom theme's eleven base colours in their groups, and the one
// picker that opens under the grid for whichever swatch is open (QBZ's
// `CustomThemeEditor.qml`). The colours are the facade's; which swatch is
// open is the only thing kept here, and it closes when the editor hides.
Column {
    id: editor

    // `{key: "#rrggbb"}`, the keys QBZ's editor uses.
    property var colors: ({})
    property string openKey: ""

    // A colour while it is dragged, a drag that ended, a HEX typed in.
    signal colorPicked(string key, string hex)
    signal released()
    signal committed(string key, string hex)

    function closePicker() { editor.openKey = "" }
    function colorOf(key) { return editor.colors[key] || "transparent" }

    onVisibleChanged: if (!visible) editor.openKey = ""

    spacing: Metrics.gap14
    topPadding: Metrics.gap8
    bottomPadding: Metrics.gap8

    component GroupLabel: Text {
        color: Theme.color.textTertiary
        font.pointSize: Type.label
        font.weight: Type.semibold
        font.letterSpacing: Type.labelTracking
    }
    component Cell: ColorCell {
        required property string key
        swatch: editor.colorOf(key)
        open: editor.openKey === key
        onActivated: editor.openKey = open ? "" : key
    }

    Row {
        spacing: Editor.groupsApart
        Column {
            spacing: Metrics.gap8
            GroupLabel { text: I18n.tr("SURFACES", I18n.rev) }
            Row {
                spacing: Metrics.gap10
                Cell { key: "surface-main"; label: I18n.tr("Background", I18n.rev) }
                Cell { key: "surface-card"; label: I18n.tr("Surface", I18n.rev) }
                Cell { key: "surface-elevated"; label: I18n.tr("Elevated", I18n.rev) }
            }
        }
        Column {
            spacing: Metrics.gap8
            GroupLabel { text: I18n.tr("TEXT", I18n.rev) }
            Row {
                spacing: Metrics.gap10
                Cell { key: "text-primary"; label: I18n.tr("Text", I18n.rev) }
                Cell { key: "text-secondary"; label: I18n.tr("Secondary text", I18n.rev) }
            }
        }
        Column {
            spacing: Metrics.gap8
            GroupLabel { text: I18n.tr("ACCENT", I18n.rev) }
            Cell { key: "accent"; label: I18n.tr("Accent", I18n.rev) }
        }
    }

    Row {
        spacing: Editor.groupsApart
        Column {
            spacing: Metrics.gap8
            GroupLabel { text: I18n.tr("STATUS", I18n.rev) }
            Row {
                spacing: Metrics.gap10
                Cell { key: "danger"; label: I18n.tr("Danger", I18n.rev) }
                Cell { key: "warning"; label: I18n.tr("Warning", I18n.rev) }
                Cell { key: "success"; label: I18n.tr("Success", I18n.rev) }
            }
        }
        Column {
            spacing: Metrics.gap8
            GroupLabel { text: I18n.tr("OTHER", I18n.rev) }
            Row {
                spacing: Metrics.gap10
                Cell { key: "border"; label: I18n.tr("Border", I18n.rev) }
                Cell { key: "favorite"; label: I18n.tr("Favorite", I18n.rev) }
            }
        }
    }

    // Kept alive while closed: it seeds itself from `value` on every change
    // anyway, and a hidden child of the Column leaves no gap.
    ColorPicker {
        objectName: "colorPicker"
        width: editor.width
        visible: editor.openKey !== ""
        value: editor.colorOf(editor.openKey)
        onPicked: (hex) => editor.colorPicked(editor.openKey, hex)
        onReleased: editor.released()
        onHexCommitted: (hex) => editor.committed(editor.openKey, hex)
    }
}
