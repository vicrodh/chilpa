import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The A-B loop beside the transport, as one picture in three layers: the
// first click sets the start and lights the A, the second sets the end and
// lights the B too, the third lets the loop go. The frame is never lit (the
// owner, 2026-10-09).
IconButton {
    id: loop
    objectName: "playerLoop"
    readonly property color rest: loop.enabled ? Theme.color.textSecondary : Theme.color.textTertiary
    enabled: Player.duration > 0
    tip: Controls.loopState === 0 ? I18n.tr("Set loop start (A)", I18n.rev) : Controls.loopState === 1 ? I18n.tr("Set loop end (B)", I18n.rev) : I18n.tr("Clear loop", I18n.rev)
    glyph: SourceGlyphs.abFrame
    iconFamily: SourceGlyphs.font
    onClicked: Controls.loopState < 2 ? Controls.markLoop() : Controls.clearLoop()

    Icon {
        objectName: "loopA"
        anchors.centerIn: parent
        glyph: SourceGlyphs.abA
        iconFamily: SourceGlyphs.font
        size: loop.iconSize
        color: Controls.loopState >= 1 ? Theme.color.accent : loop.rest
    }
    Icon {
        objectName: "loopB"
        anchors.centerIn: parent
        glyph: SourceGlyphs.abB
        iconFamily: SourceGlyphs.font
        size: loop.iconSize
        color: Controls.loopState === 2 ? Theme.color.accent : loop.rest
    }
}
