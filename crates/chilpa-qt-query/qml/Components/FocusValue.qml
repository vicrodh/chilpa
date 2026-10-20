import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Row {
    id: chip
    property string text
    property bool excluded: false
    signal toggled()
    signal removed()
    spacing: Metrics.gap2
    PillButton {
        objectName: "focusValueToggle"
        text: chip.text
        glyph: chip.excluded ? Icons.minus : ""
        kind: chip.excluded ? "secondary" : "primary"
        onClicked: chip.toggled()
    }
    IconButton {
        objectName: "focusValueRemove"
        glyph: Icons.x
        tip: I18n.tr("Remove", I18n.rev)
        onClicked: chip.removed()
    }
}
