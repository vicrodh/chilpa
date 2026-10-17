import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/playlists/qml/Components"

TestCase {
    name: "SourceFolders"; when:windowShown; visible:true; width:960; height:600
    PlaylistFolderDialog { id: editor }
    function init() {
        Playlists.fail=false; Playlists.created=[]; Sources.asked=[]
        Sources.show({native_actions:{}})
        Playlists.catalog={folders:[{id:1,name:"Local"}],folder_sources:[{id:"fixture",name:"Cloud"}],
            native_folders:[{source:"fixture",ref:"folder:1",name:"Native"}]}
    }
    function cleanup() { editor.request=""; editor.close() }
    function test_create_native_folder_waits_for_confirmation() {
        editor.show(0,"New","accent"); editor.provider="fixture"; editor.destination="native:folder:1"
        editor.save()
        const sent=Sources.asked[0], input=JSON.parse(sent[2])
        compare(sent.slice(0,2),["native","fixture"])
        compare(input.action,"folder.create"); compare(input.parent,"folder:1"); compare(input.title,"New")
        verify(editor.visible)
        Sources.show({native_actions:{fixture:{request_id:editor.request,pending:false,error:"Rejected"}}})
        compare(editor.notice,"Rejected"); verify(editor.visible)
        Sources.show({native_actions:{fixture:{request_id:editor.request,pending:false}}})
        tryCompare(editor,"visible",false)
    }
    function test_native_move_keeps_remote_identity_and_clears_local_placement_only_after_success() {
        editor.move({playlistId:-8,nativeRef:"playlist:8",source:"fixture",folderId:1})
        verify(editor.destinations.some(d=>d.id==="local:1"))
        verify(editor.destinations.some(d=>d.id==="native:folder:1"))
        editor.destination="native:folder:1"; editor.save()
        const input=JSON.parse(Sources.asked[0][2])
        compare(input.action,"playlist.folder"); compare(input.ref,"playlist:8"); compare(input.folder,"folder:1")
        compare(Playlists.created.length,0)
        Sources.show({native_actions:{fixture:{request_id:editor.request,pending:false}}})
        compare(Playlists.created,[-8,0])
    }
    function test_local_move_never_calls_remote_write() {
        editor.move({playlistId:-8,nativeRef:"playlist:8",source:"fixture",folderId:0})
        editor.destination="local:1"; editor.save()
        compare(Playlists.created,[-8,1]); compare(Sources.asked.length,0)
    }
    function test_read_only_account_folders_keep_local_moves_available() {
        Playlists.catalog={folders:[{id:1,name:"Local"}],folder_sources:[{id:"fixture",name:"Cloud",editable:false}],
            native_folders:[{source:"fixture",ref:"folder:1",name:"Native",editable:false}]}
        editor.move({playlistId:-8,nativeRef:"playlist:8",source:"fixture",folderId:0,nativeFolder:"folder:1"})
        compare(editor.provider,""); verify(editor.nativeReadOnly)
        compare(editor.providers.map(p=>p.id),[""])
        compare(editor.destinations.map(p=>p.id),["local:0","local:1"])
        editor.destination="local:1"; editor.save()
        compare(Playlists.created,[-8,1]); compare(Sources.asked.length,0)
        editor.showNative({source:"fixture",nativeRef:"folder:1",editable:false})
        verify(!editor.visible)
    }
}
