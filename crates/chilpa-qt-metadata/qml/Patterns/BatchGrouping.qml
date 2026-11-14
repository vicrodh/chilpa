import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id:form
    property var groups: []
    property var discs: []
    property bool busy: false
    property int dataRevision: 0
    property string operation: "join"
    signal proposed(var plan)
    spacing:Metrics.gap8
    function reset() {
        const result=[]
        for (const group of groups) for (const source of group.discs?.length ? group.discs : [{number:0}]) {
            result.push({groupKey:group.key,sourceDisc:source.number || 0,number:operation === "split" ? source.number || 1 : result.length + 1,title:"",album:group.title || "",trackNumberMode:"keep"})
        }
        const unique=result.every(disc=>disc.sourceDisc > 0) && new Set(result.map(disc=>disc.sourceDisc)).size === result.length
        discs=result.map((disc,index)=>Object.assign({},disc,{number:operation === "join" && !unique ? index + 1 : disc.sourceDisc || 1}))
        album.text=groups[0]?.title || "";artist.text=groups[0]?.artist || ""
    }
    function update(index,field,value) { const next=discs.slice();next[index]=Object.assign({},next[index],{[field]:value});discs=next }
    function propose() {
        const plan={operation:operation,dataRevision:dataRevision,groups:groups.map(group=>group.key),targets:{mode:"selected"},discs:discs.map(disc=>Object.assign({},disc))}
        if (operation === "join") {
            plan.album=album.text;plan.albumArtist=artist.text
            for (const disc of plan.discs) delete disc.album
        } else {
            plan.renumberDiscs=renumber.checked
            for (const disc of plan.discs) delete disc.title
        }
        form.proposed(plan)
    }
    onGroupsChanged:Qt.callLater(reset)
    onOperationChanged:Qt.callLater(reset)
    Component.onCompleted:reset()
    Text { width:parent.width; text:I18n.tr("Disc plan",I18n.rev); font.pointSize:Type.section; color:Theme.color.textPrimary }
    Text { width:parent.width; text:I18n.tr("Choose albums with Use in disc plan. Only selected files participate.",I18n.rev); wrapMode:Text.Wrap; font.pointSize:Type.meta; color:Theme.color.textSecondary }
    SelectField {
        width:parent.width; enabled:!form.busy
        values:["join","split"]; options:[I18n.tr("Join as a multidisc album",I18n.rev),I18n.tr("Separate by disc",I18n.rev)];value:form.operation
        onPicked:value=>form.operation=value
    }
    TextField { id:album; objectName:"batchJoinTitle"; width:parent.width; visible:form.operation === "join"; enabled:!form.busy; placeholder:I18n.tr("Album title",I18n.rev) }
    TextField { id:artist; objectName:"batchJoinArtist"; width:parent.width; visible:form.operation === "join"; enabled:!form.busy; placeholder:I18n.tr("Album artist",I18n.rev) }
    Repeater {
        model:form.discs.length <= 32 ? form.discs.length : 0
        Column {
            required property int index
            readonly property var disc:form.discs[index]
            width:form.width; spacing:Metrics.gap6
            Text { width:parent.width; text:I18n.trArgs("{} · disc {}",[form.groups.find(group=>group.key === disc.groupKey)?.title || "",disc.sourceDisc],I18n.rev); textFormat:Text.PlainText; wrapMode:Text.Wrap; color:Theme.color.textSecondary; font.pointSize:Type.meta }
            TextField { width:parent.width; enabled:!form.busy; text:String(disc.number); placeholder:I18n.tr("Disc number",I18n.rev); onTextEdited:form.update(index,"number",Number(text)) }
            TextField { width:parent.width; enabled:!form.busy; visible:form.operation === "join"; text:disc.title; placeholder:I18n.tr("Disc subtitle",I18n.rev); onTextEdited:form.update(index,"title",text) }
            TextField { width:parent.width; enabled:!form.busy; visible:form.operation === "split"; text:disc.album; placeholder:I18n.tr("Resulting album title",I18n.rev); onTextEdited:form.update(index,"album",text) }
            SelectField { width:parent.width; enabled:!form.busy; values:["keep","sequence"]; options:[I18n.tr("Keep track numbers",I18n.rev),I18n.tr("Number tracks in order",I18n.rev)]; value:disc.trackNumberMode; onPicked:value=>form.update(index,"trackNumberMode",value) }
        }
    }
    Flow {
        width:parent.width; spacing:Metrics.gap8; visible:form.operation === "split"
        Toggle { id:renumber; compact:true; enabled:!form.busy; Accessible.name:I18n.tr("Renumber resulting discs",I18n.rev) }
        Text { text:I18n.tr("Renumber resulting discs",I18n.rev); font.pointSize:Type.meta; color:Theme.color.textSecondary }
    }
    Text { width:parent.width; visible:form.discs.length > 32; text:I18n.tr("Choose up to 32 discs for one plan.",I18n.rev); wrapMode:Text.Wrap; font.pointSize:Type.meta; color:Theme.color.statusWarning }
    PillButton { objectName:"batchPreviewGrouping"; text:I18n.tr("Preview disc plan",I18n.rev); enabled:!form.busy && form.discs.length > 0 && form.discs.length <= 32; onClicked:form.propose() }
}
