import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.metadata
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Patterns"
import "../Components"

Item {
    id: page
    property string routeKey: "[]"
    readonly property var doc: { try { return JSON.parse(batch.document || "{}") } catch (_) { return {} } }
    readonly property bool ready: doc.state === "ready" && doc.beets?.ready === true
    property var grouped: []
    property var candidate: ({})
    property var activeGroup: ({})
    property string seenPlan: ""
    property bool returnToFiles: false
    property string groupingError: ""
    function select(selection) { if (batch.groupKey) selection.groupKey=batch.groupKey;batch.select(JSON.stringify(selection)) }
    function patch(patch) { if (batch.groupKey) patch.groupKey=batch.groupKey;batch.patch(JSON.stringify(patch)) }
    function browse(kind,group) { activeGroup=group || ({}); batch.browse(kind,kind === "items" ? activeGroup.key || "" : "") }
    function groupChosen(group) {
        const compact={key:group.key,title:group.title,artist:group.artist,discs:(group.discs || []).map(disc=>({number:disc.number}))}
        const next=grouped.some(value=>value.key === group.key) ? grouped.filter(value=>value.key !== group.key) : grouped.concat([compact])
        if (next.length > 32 || next.reduce((sum,value)=>sum + Math.max(1,value.discs.length),0) > 32 || JSON.stringify(next).length > 524288) {
            groupingError=I18n.tr("Choose a smaller selection, up to 32 discs for one plan.",I18n.rev);return
        }
        groupingError="";grouped=next
    }
    function chooseCandidate(value,fields,genresOperation) {
        candidate=value
        batch.candidate(JSON.stringify({candidateKey:value.key,groupKey:value.groupKey,dataRevision:doc.dataRevision,targets:{mode:"selected"},fields:fields || [],genresOperation:genresOperation || "append_unique"}))
    }
    function settled() {
        if (batch.busy) return
        if (returnToFiles) { returnToFiles=false;browse("items");return }
        const plan=doc.plan?.id || ""
        if (plan && plan !== seenPlan) { seenPlan=plan;browse("plan") }
    }
    function leave() { batch.close(); Navigation.canBack ? Navigation.back() : Navigation.go("folders",0) }
    function askClose() { doc.dirty || batch.busy ? discard.open() : leave() }
    Component.onCompleted: batch.openPaths(routeKey)
    MetadataBatch { id:batch; objectName:"folderMetadataModel"; active:page.visible; kind:"items" }
    Connections {
        target:batch
        function onDocumentChanged() { Qt.callLater(page.settled) }
        function onBusyChanged() { Qt.callLater(page.settled) }
    }
    PulseClock { slow:true; minimumInterval:100; running:page.visible && batch.busy; onTriggered:batch.poll() }
    Column {
        id:heading
        x:Metrics.contentX; y:Metrics.contentTop
        width:page.width - Metrics.contentX * 2 // derived: screen margins
        spacing:Metrics.gap8
        Text { text:I18n.tr("Folder metadata", I18n.rev); font.pointSize:Type.title; font.weight:Type.bold; color:Theme.color.textPrimary }
        Text { width:parent.width; text:I18n.trCount("{} selected file","{} selected files",page.doc.counts?.selected || 0,I18n.rev); color:Theme.color.textSecondary; font.pointSize:Type.body; wrapMode:Text.Wrap }
        Text { width:parent.width; visible:page.doc.state === "opening"; text:I18n.tr("Reading folder metadata…",I18n.rev); color:Theme.color.textSecondary; font.pointSize:Type.meta; wrapMode:Text.Wrap }
        BatchProgress {
            width: parent.width
            progress: { try { return JSON.parse(batch.progress || "{}") } catch (_) { return {} } }
        }
        Text { width:parent.width; visible:(page.doc.summary?.changed || 0) > 0; text:I18n.trCount("{} file has pending changes","{} files have pending changes",page.doc.summary?.changed || 0,I18n.rev); color:Theme.color.textSecondary; font.pointSize:Type.meta; wrapMode:Text.Wrap }
        Text { objectName:"batchProtected"; width:parent.width; visible:page.doc.protected === true; text:I18n.tr("Protect my library is on. Turn it off in Settings > Library to change your files or servers.", I18n.rev); color:Theme.color.textSecondary; font.pointSize:Type.meta; wrapMode:Text.Wrap }
        Flow {
            width:parent.width; spacing:Metrics.gap8
            PillButton { objectName:"batchSave"; text:I18n.tr("Write reviewed changes", I18n.rev); kind:"primary"; enabled:page.ready && !batch.busy && (page.doc.summary?.changed || 0) > 0 && page.doc.protected !== true; onClicked:write.open() }
            PillButton { text:I18n.tr("Retry pending files", I18n.rev); visible:(page.doc.summary?.failed || 0) > 0; enabled:!batch.busy; onClicked:batch.retry() }
            PillButton { text:I18n.tr("Cancel", I18n.rev); visible:batch.busy; onClicked:batch.cancel() }
            PillButton { text:I18n.tr("Close", I18n.rev); onClicked:page.askClose() }
            PillButton { objectName:"batchReopen";text:I18n.tr("Open the selection again",I18n.rev);visible:["unavailable","failed","closed"].includes(page.doc.state);enabled:!batch.busy;onClicked:batch.openPaths(page.routeKey) }
            PillButton { text:I18n.tr("Settings", I18n.rev); visible:page.doc.beets?.ready !== true; onClicked:Navigation.goKey("settings","library") }
        }
        Text { width:parent.width; visible:!!page.doc.error || page.doc.beets?.ready !== true; text:page.doc.error || page.doc.beets?.reason || I18n.tr("Install or check beets in Settings > Library to use folder metadata.", I18n.rev); wrapMode:Text.Wrap; color:page.doc.error || !page.doc.beets?.busy ? Theme.color.statusNegative : Theme.color.textSecondary; font.pointSize:Type.body }
        Text { width:parent.width; visible:!!page.doc.notice; text:page.doc.notice || ""; textFormat:Text.PlainText; wrapMode:Text.Wrap; color:Theme.color.textSecondary; font.pointSize:Type.meta }
        Text { width:parent.width;visible:page.doc.state === "failed" && !!page.doc.lastWrittenFile?.path;text:I18n.trArgs("Last file changed: {}",[page.doc.lastWrittenFile?.path || ""],I18n.rev);textFormat:Text.PlainText;elide:Text.ElideMiddle;color:Theme.color.textSecondary;font.pointSize:Type.meta }
        Text { width:parent.width;visible:page.doc.state === "failed" && page.doc.lastWrittenFile?.fileWritten === true;text:I18n.tr("The file tags have been written.",I18n.rev);color:Theme.color.textSecondary;font.pointSize:Type.meta;wrapMode:Text.Wrap }
        Text { width:parent.width;visible:page.doc.state === "failed" && page.doc.lastWrittenFile?.sidecarWritten === true;text:I18n.tr("The folder metadata has been saved.",I18n.rev);color:Theme.color.textSecondary;font.pointSize:Type.meta;wrapMode:Text.Wrap }
        Text { width:parent.width; visible:!!page.groupingError; text:page.groupingError; wrapMode:Text.Wrap; color:Theme.color.statusWarning; font.pointSize:Type.meta }
        Text { width:parent.width; visible:(page.doc.summary?.saved || 0) > 0; text:I18n.trCount("{} file saved","{} files saved",page.doc.summary?.saved || 0,I18n.rev); color:Theme.color.textSecondary; font.pointSize:Type.body }
    }
    Row {
        anchors.left:heading.left; anchors.right:heading.right; anchors.top:heading.bottom; anchors.topMargin:Metrics.gap16
        anchors.bottom:parent.bottom; anchors.bottomMargin:Metrics.playerSpace + Metrics.playerBottom * 2; spacing:Metrics.gap16 // derived: clear of the player bar
        Flickable {
            id:tools
            width:Math.min(Metrics.metadataColumnMin,parent.width / 3); height:parent.height // derived: leave two thirds for previews
            contentHeight:form.implicitHeight; clip:true; boundsBehavior:Flickable.StopAtBounds
            ScrollBar.vertical:ViewportScrollBar {}
            Column {
                id:form; width:tools.width - Metrics.gap8; spacing:Metrics.gap16 // derived: scrollbar gutter
                BatchSelection { width:parent.width; busy:batch.busy || !page.ready; onChosen:selection=>page.select(selection) }
                BatchPatch { width:parent.width; visible:batch.kind === "items"; busy:batch.busy || !page.ready; fields:page.doc.fields || []; selected:page.doc.counts?.selected || 0; onProposed:patch=>page.patch(patch) }
                BatchGrouping { objectName:"batchGrouping"; width:parent.width; visible:batch.kind === "groups"; groups:page.grouped; busy:batch.busy || !page.ready; dataRevision:page.doc.dataRevision || 0; onProposed:plan=>batch.groupPlan(JSON.stringify(plan)) }
                BatchLookup { width:parent.width; visible:batch.kind === "groups"; lookup:page.doc.lookup || ({}); busy:batch.busy || !page.ready; onChosen:value=>page.chooseCandidate(value,[]) }
                BatchProposal {
                    objectName:"batchProposal";width:parent.width; visible:!!page.doc.plan?.id; plan:page.doc.plan || ({});busy:batch.busy || !page.ready;dataRevision:page.doc.dataRevision || 0
                    onFieldsChosen:(fields,genresOperation)=>page.chooseCandidate(page.candidate,fields,genresOperation)
                    onApplied:id=> { page.returnToFiles=batch.applyPlan(id) }
                }
            }
        }
        Column {
            width:parent.width - tools.width - parent.spacing; height:parent.height;spacing:Metrics.gap8
            SegmentedControl {
                objectName:"batchBrowseMode"; enabled:!batch.busy
                options:[{text:I18n.tr("Files",I18n.rev)},{text:I18n.tr("Albums",I18n.rev)},{text:I18n.tr("Proposal",I18n.rev),disabled:!page.doc.plan?.id}]
                current:["items","groups","plan"].indexOf(batch.kind)
                onActivated:index=>page.browse(["items","groups","plan"][index])
            }
            Flow {
                id:scope; width:parent.width; spacing:Metrics.gap8;visible:!!batch.groupKey
                Text { text:page.activeGroup.title || ""; textFormat:Text.PlainText; color:Theme.color.textPrimary; font.pointSize:Type.body }
                PillButton { text:I18n.tr("All files",I18n.rev);enabled:!batch.busy;onClicked:page.browse("items") }
            }
            BatchPreview {
                width:parent.width; height:parent.height - y
                batch:batch; hardlinkPolicy:page.doc.hardlinkPolicy || "";grouped:page.grouped
                onGroupOpened:group=>page.browse("items",group)
                onLookupChosen:group=>batch.lookupGroup(group.key)
                onGroupingChosen:group=>page.groupChosen(group)
            }
        }
    }
    ConfirmDialog {
        id:write
        title:I18n.tr("Write reviewed changes?", I18n.rev)
        message:I18n.tr("All selected files with pending changes will be saved, including files outside the current view. Only reviewed fields will change. Completed writes cannot be undone from Chilpa. Files are not moved or renamed.", I18n.rev)
        confirmText:I18n.tr("Write into the files", I18n.rev)
        destructive:true; doubleConfirmation:true
        onConfirmed:batch.save()
    }
    ConfirmDialog {
        id:discard
        title:I18n.tr("Close folder metadata?", I18n.rev)
        message:I18n.tr("Pending changes will be discarded. Files already saved keep their changes.", I18n.rev)
        confirmText:I18n.tr("Close", I18n.rev); destructive:true
        onConfirmed:page.leave()
    }
}
