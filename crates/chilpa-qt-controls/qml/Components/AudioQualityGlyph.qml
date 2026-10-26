import QtQuick
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// I extend only the player glyph; quality icons in track rows stay passive.
QualityIcon {
    id: quality
    property bool preview: false
    readonly property bool hovered: over.hovered
    activeFocusOnTab: visible
    Accessible.role: Accessible.Button
    Accessible.name: I18n.tr("Audio and output", I18n.rev)
    function showNow() { if (!preview) AudioOutput.show("now") }
    Keys.onReturnPressed: showNow()
    Keys.onEnterPressed: showNow()
    Keys.onSpacePressed: showNow()
    TapHandler { margin: Metrics.gap8; onTapped: { quality.forceActiveFocus(); quality.showNow() } }
    HoverHandler { id: over; cursorShape: Qt.PointingHandCursor }
    FocusRing {}
}
