import QtQuick
import QtTest
import "../../qml/Components/PlaylistTree.js" as Tree

TestCase {
    name: "PlaylistTree"
    function test_native_hierarchy_remains_separate_from_local_folders() {
        const catalog = {folders:[{id:1,name:"Local",playlist_count:1,color:"accent",position:0}],
            folder_sources:[{id:"source",name:"Cloud",icon:""}],
            native_folders:[{source:"source",ref:"folder:p",name:"Parent",parent:"",source_icon:"source.svg",folder_icon:"folder.svg"},{source:"source",ref:"folder:c",name:"Child",parent:"folder:p"}],
            playlists:[{id:-1,name:"Inside",source:"source",provider:"Cloud",native_ref:"playlist:1",native_folder:"folder:c",folder_id:0,separate:true,mixed:false},
                {id:-2,name:"Locally organized",source:"source",provider:"Cloud",native_folder:"folder:c",folder_id:1,separate:true,mixed:false}]}
        const rows = Tree.rows(catalog,"name",true,"",{1:true,"source:source":true,"source folder:p":true,"source folder:c":true})
        compare(rows.map(r=>r.name),["Local","Locally organized","Cloud","Parent","Child","Inside"])
        compare(rows[3].sourceIcon,"folder.svg")
        compare(rows[4].sourceIcon,"")
        compare(rows[1].depth,1)
        compare(rows[4].nativeRef,"folder:c")
        compare(rows[5].nativeRef,"playlist:1")
        compare(rows[5].depth,3)
        verify(rows[1].showSourceBadge)
        verify(!rows[5].showSourceBadge)
    }
    function test_empty_native_folder_stays_visible_and_expandable() {
        const rows = Tree.rows({folder_sources:[{id:"source",name:"Cloud"}],native_folders:[{source:"source",ref:"folder:e",name:"Empty"}]},"name",true,"",{"source:source":true})
        compare(rows.length,2)
        compare(rows[1].name,"Empty")
        compare(rows[1].tracks,0)
        compare(rows[1].expandKey,"source folder:e")
    }
}
