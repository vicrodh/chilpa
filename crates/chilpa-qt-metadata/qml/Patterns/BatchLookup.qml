import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"

Column {
    id:root
    property var lookup: ({})
    property bool busy: false
    signal chosen(var candidate)
    spacing:Metrics.gap8
    Text { width:parent.width; text:I18n.tr("beets matches",I18n.rev); font.pointSize:Type.section; color:Theme.color.textPrimary }
    Text { width:parent.width; text:root.lookup.error || I18n.tr("Choose an album and look it up. Choosing a match prepares a proposal without writing files.",I18n.rev); wrapMode:Text.Wrap; font.pointSize:Type.meta; color:root.lookup.error ? Theme.color.statusNegative : Theme.color.textSecondary }
    Repeater {
        model:(root.lookup.candidates || []).slice(0,5)
        ReleaseHitRow {
            required property var modelData
            width:root.width; hit:modelData.hit || ({}); showScore:true; enabled:!root.busy
            onChoose:root.chosen(modelData)
        }
    }
}
