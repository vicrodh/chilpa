import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"

Modal {
    id: dialog
    objectName: "playlistFolderDialog"
    property int folderId: 0
    property string colorName: "accent"
    property string notice: ""
    property string provider: ""
    property string reference: ""
    property string mode: "folder"
    property var playlist: ({})
    property var catalog: ({})
    property string destination: "local:0"
    property string request: ""
    readonly property var state: (JSON.parse(Sources.snapshot || "{}").native_actions || {})[provider] || ({})
    readonly property bool pending: !!request && state.pending === true
    readonly property var providers: [{id:"",name:I18n.tr("Local — Chilpa", I18n.rev)}].concat((catalog.folder_sources || []).map(p => ({id:p.id,name:I18n.trArgs("Native — {}",[p.name],I18n.rev)})))
    readonly property var destinations: {
        const rows = mode === "move" ? [{id:"local:0",name:I18n.tr("Local — No folder",I18n.rev)}].concat((catalog.folders || []).map(f => ({id:"local:"+f.id,name:I18n.trArgs("Local — {}",[f.name],I18n.rev)}))) : []
        if (provider) {
            const service = (catalog.folder_sources || []).find(p => p.id === provider)
            const label = service ? service.name : provider
            rows.push({id:"native:",name:I18n.trArgs("Native — {} — No folder",[label],I18n.rev)})
            const folders = (catalog.native_folders || []).filter(f => f.source === provider)
            function path(f,seen) { if (!f.parent || seen.indexOf(f.ref)>=0) return f.name; const parent=folders.find(p=>p.ref===f.parent); return parent ? path(parent,seen.concat(f.ref))+" / "+f.name : f.name }
            for (const f of folders) rows.push({id:"native:"+f.ref,name:I18n.trArgs("Native — {} — {}",[label,path(f,[])],I18n.rev)})
        }
        return rows
    }
    width: Math.max(Metrics.modalInsets, Math.min(Metrics.playlistFolderDialog, parent.width - Metrics.modalInset))
    closePolicy: pending ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnPressOutside
    function show(id,name,color) {
        catalog = JSON.parse(Playlists.catalogJson(""))
        mode="folder"; folderId=id; nameField.text=name; colorName=color; notice=""; provider=""; reference=""; request=""; destination="native:"
        open(); Qt.callLater(nameField.focusInput)
    }
    function showNative(row) { show(0,row.name,"accent"); provider=row.source; reference=row.nativeRef }
    function move(row) {
        show(0,"","accent"); mode="move"; playlist=row
        provider=(catalog.folder_sources || []).some(p=>p.id===row.source) ? row.source : ""
        destination=row.folderId>0 ? "local:"+row.folderId : provider && row.nativeFolder ? "native:"+row.nativeFolder : "local:0"
    }
    function save() {
        if (pending) return
        if (mode === "move" && destination.startsWith("local:")) {
            if (Playlists.moveToFolder(playlist.playlistId,Number(destination.slice(6)))) close()
            else notice=I18n.tr("Could not move the playlist.",I18n.rev)
            return
        }
        if (mode === "folder" && !provider) {
            const saved=folderId>0 ? Playlists.updateFolder(folderId,nameField.text,colorName) : Playlists.createFolder(nameField.text,colorName)>0
            if (saved) close(); else notice=I18n.tr("Could not save the folder. Please try again.",I18n.rev)
            return
        }
        request=String(Date.now())+":folder"
        const input=mode === "move" ? {action:"playlist.folder",ref:playlist.nativeRef,folder:destination.slice(7)}
            : reference ? {action:"folder.rename",ref:reference,title:nameField.text} : {action:"folder.create",title:nameField.text,parent:destination.slice(7)}
        input.request_id=request
        Sources.command("native",provider,JSON.stringify(input))
    }
    onStateChanged: {
        if (!request || state.request_id !== request || state.pending) return
        if (state.error) { notice=state.error; return }
        if (mode === "move") Playlists.moveToFolder(playlist.playlistId,0)
        Playlists.refresh(); close()
    }
    Connections { target: Library; function onRemoteRevChanged() { if (dialog.visible) dialog.catalog = JSON.parse(Playlists.catalogJson("")) } }
    Timer { interval: 300; running: dialog.visible && dialog.pending; repeat:true; onTriggered: Sources.refresh() }
    Column {
        width: parent.width; spacing: Metrics.gap12
        Text { width:parent.width; text:dialog.mode === "move" ? I18n.tr("Move to folder",I18n.rev) : dialog.folderId>0 || dialog.reference ? I18n.tr("Edit folder",I18n.rev) : I18n.tr("New folder",I18n.rev); font.pointSize:Type.titleSmall; font.weight:Type.bold; color:Theme.color.textPrimary; wrapMode:Text.WordWrap }
        SelectField {
            width:parent.width; visible:dialog.mode === "folder" && !dialog.folderId && !dialog.reference; enabled:!dialog.pending
            options:dialog.providers.map(p=>p.name); values:dialog.providers.map(p=>p.id); value:dialog.provider
            onPicked:value=>{dialog.provider=value;dialog.destination="native:"}
        }
        TextField { id:nameField; objectName:"folderName"; visible:dialog.mode === "folder"; width:parent.width; enabled:!dialog.pending; placeholder:I18n.tr("Folder name",I18n.rev); onAccepted:dialog.save() }
        SelectField {
            width:parent.width; visible:dialog.mode === "move" || !!dialog.provider && !dialog.reference; enabled:!dialog.pending
            options:dialog.destinations.map(p=>p.name); values:dialog.destinations.map(p=>p.id); value:dialog.destination; onPicked:value=>dialog.destination=value
        }
        Text { width:parent.width; visible:dialog.mode === "move"; text:I18n.tr("Local folders organize this sidebar. Native folders are saved in your service account.",I18n.rev); color:Theme.color.textSecondary; font.pointSize:Type.small; wrapMode:Text.WordWrap }
        Row {
            visible:dialog.mode === "folder" && !dialog.provider; spacing:Metrics.gap8
            Repeater {
                model:["accent","positive","warning","negative"]
                Rectangle {
                    required property string modelData
                    width:Metrics.colorSwatch; height:width; radius:Metrics.colorSwatchRadius
                    color:modelData === "positive" ? Theme.color.statusPositive : modelData === "warning" ? Theme.color.statusWarning : modelData === "negative" ? Theme.color.statusNegative : Theme.color.accent
                    border.width:dialog.colorName === modelData ? Metrics.focusRing : 0; border.color:Theme.color.textPrimary
                    HoverHandler { cursorShape:Qt.PointingHandCursor }
                    TapHandler { onTapped:dialog.colorName=parent.modelData }
                }
            }
        }
        Text { width:parent.width; visible:!!dialog.notice; text:dialog.notice; wrapMode:Text.WordWrap; font.pointSize:Type.small; color:Theme.color.statusWarning }
        Row {
            anchors.right:parent.right; spacing:Metrics.gap8
            PillButton { text:I18n.tr("Cancel",I18n.rev); enabled:!dialog.pending; onClicked:dialog.close() }
            PillButton { objectName:"saveFolder"; kind:"primary"; text:dialog.mode === "move" ? I18n.tr("Move",I18n.rev) : I18n.tr("Save",I18n.rev); enabled:!dialog.pending && (dialog.mode === "move" || !!nameField.text.trim()); onClicked:dialog.save() }
        }
    }
}
