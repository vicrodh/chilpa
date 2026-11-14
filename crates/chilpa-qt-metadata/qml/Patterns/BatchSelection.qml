import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: selection
    property bool busy: false
    signal chosen(var selection)
    spacing:Metrics.gap8
    Text { text:I18n.tr("Files to change", I18n.rev); color:Theme.color.textPrimary; font.pointSize:Type.body }
    Flow {
        width:parent.width; spacing:Metrics.gap8
        PillButton { text:I18n.tr("Select all", I18n.rev); enabled:!selection.busy; onClicked:selection.chosen({mode:"all",selected:true}) }
        PillButton { text:I18n.tr("Clear selection", I18n.rev); enabled:!selection.busy; onClicked:selection.chosen({mode:"all",selected:false}) }
    }
    MultilineField {
        id: artists; objectName:"batchArtists"
        width:parent.width; height:Metrics.fieldHeightLarge * 2 // derived: room for two artists
        enabled:!selection.busy; placeholder:I18n.tr("Artist names, one per line", I18n.rev)
    }
    PillButton {
        objectName:"batchChooseArtists"
        text:I18n.tr("Select these artists", I18n.rev); enabled:!selection.busy && artists.text.trim() !== ""
        onClicked:selection.chosen({mode:"artists",artists:artists.text.split(/\r?\n/).map(name=>name.trim()).filter(Boolean),selected:true,replace:true})
    }
}
