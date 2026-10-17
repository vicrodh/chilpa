import QtQuick
import QtTest
import app.chilpa
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/qml/Components/PlaylistTree.js" as Tree

TestCase {
    name: "PlaylistTree"; when: windowShown; visible: true; width: 400; height: 1000
    Component { id: sidebarMaker; SidebarPlaylists { width: 300; height: 1000 } }
    function fixture(separateSources) {
        const providers = ["q", "t", "a"].map((id,index) => ({id:id,name:"Source " + id,icon:id + ".svg",position:index,separate:separateSources.includes(id)}))
        const native = []
        for (const id of ["q", "t", "a"])
            for (const n of id === "q" ? [1] : [1,2]) native.push({source:id,ref:"folder:" + n,name:"Folder " + n,parent:"",source_icon:id + ".svg"})
        const playlists = [], counts = {A:["", "q", "t", "a"],B:["", "q", "t", "a"],C:["", "q", "t"],D:["", "q", "t"],E:["", "t"]}
        let index = 1
        for (const name of ["A", "B", "C", "D", "E"])
            for (const source of counts[name]) playlists.push({id:source ? -index++ : index++,name:"Playlist " + name,source:source,
                provider:source || "Local",mixed:!source,source_icon:source + ".svg",separate:separateSources.includes(source),track_count:1})
        return {folders:[{id:1,name:"Folder 1"},{id:2,name:"Folder 2"}],native_folders:native,folder_sources:providers,playlists:playlists}
    }
    function test_all_integrated_interleave_folders_then_playlists_by_name_and_origin() {
        const rows = Tree.rows(fixture([]), "name", true, "", {})
        compare(rows.filter(r => r.rowKind === 2).length, 0)
        compare(rows.slice(0,7).map(r => r.name + ":" + r.source),
            ["Folder 1:","Folder 1:q","Folder 1:t","Folder 1:a","Folder 2:","Folder 2:t","Folder 2:a"])
        compare(rows.slice(7,11).map(r => r.name + ":" + r.source), ["Playlist A:","Playlist A:q","Playlist A:t","Playlist A:a"])
        compare(rows.slice(-2).map(r => r.name + ":" + r.source), ["Playlist E:","Playlist E:t"])
        compare(new Set(rows.filter(r => r.rowKind === 0).map(r => r.playlistId)).size, 16)
        verify(rows.filter(r => r.rowKind === 0).every(r => r.showSourceBadge))
    }
    function test_partial_integration_keeps_other_sources_in_their_own_groups() {
        const rows = Tree.rows(fixture(["t", "a"]), "name", true, "", {})
        const groups = rows.filter(r => r.rowKind === 2)
        compare(groups.map(r => r.source), ["t", "a"])
        const t = rows.indexOf(groups[0]), a = rows.indexOf(groups[1])
        verify(rows.slice(0,t).every(r => !r.source || r.source === "q"))
        compare(rows.slice(t+1,a).map(r => r.name), ["Folder 1","Folder 2","Playlist A","Playlist B","Playlist C","Playlist D","Playlist E"])
        compare(rows.slice(a+1).map(r => r.name), ["Folder 1","Folder 2","Playlist A","Playlist B"])
        compare(rows.filter(r => r.rowKind === 0).length, 16)
    }
    function test_separate_groups_render_with_source_identity_in_the_sidebar() {
        const catalog = fixture(["t", "a"])
        for (const provider of catalog.folder_sources) provider.icon = ""
        for (const folder of catalog.native_folders) folder.source_icon = ""
        for (const playlist of catalog.playlists) playlist.source_icon = ""
        Playlists.catalog = catalog
        const sidebar = createTemporaryObject(sidebarMaker, this)
        sidebar.setOrder("name")
        waitForRendering(sidebar)
        const group = findChild(sidebar, "playlistSourceGroup")
        verify(group !== null)
        compare(group.title, "Playlists")
        compare(group.sourceName, "Source t")
        verify(group.visible)
        compare(sidebar.rows.filter(row => row.rowKind === 2).map(row => row.source), ["t", "a"])
    }
    function test_empty_integrated_folder_and_orphan_members_remain_visible() {
        const catalog = {folder_sources:[{id:"source",name:"Catalog",separate:false}],
            native_folders:[{source:"source",ref:"folder:empty",name:"Empty"}],
            playlists:[{id:-1,source:"source",name:"Orphan",native_folder:"folder:missing",separate:false}]}
        compare(Tree.rows(catalog,"name",true,"",{}).map(r => r.name), ["Empty","Orphan"])
    }
    function test_native_hierarchy_remains_separate_from_local_folders() {
        const catalog = {folders:[{id:1,name:"Local",playlist_count:1,color:"accent",position:0}],
            folder_sources:[{id:"source",name:"Cloud",icon:""}],
            native_folders:[{source:"source",ref:"folder:p",name:"Parent",parent:"",source_icon:"source.svg",folder_icon:"folder.svg"},{source:"source",ref:"folder:c",name:"Child",parent:"folder:p"}],
            playlists:[{id:-1,name:"Inside",source:"source",provider:"Cloud",native_ref:"playlist:1",native_folder:"folder:c",folder_id:0,separate:true,mixed:false},
                {id:-2,name:"Locally organized",source:"source",provider:"Cloud",native_folder:"folder:c",folder_id:1,separate:true,mixed:false}]}
        const rows = Tree.rows(catalog,"name",true,"",{1:true,"source local:1":true,"source:source":true,"source folder:p":true,"source folder:c":true})
        compare(rows.map(r=>r.name),["Local","Playlists","Local","Locally organized","Parent","Child","Inside"])
        compare(rows[4].sourceIcon,"folder.svg")
        compare(rows[5].sourceIcon,"")
        compare(rows[3].depth,1)
        compare(rows[5].nativeRef,"folder:c")
        compare(rows[6].nativeRef,"playlist:1")
        compare(rows[6].depth,2)
        verify(rows[3].showSourceBadge)
        verify(rows[6].showSourceBadge)
    }
    function test_empty_native_folder_stays_visible_and_expandable() {
        const rows = Tree.rows({folder_sources:[{id:"source",name:"Cloud"}],native_folders:[{source:"source",ref:"folder:e",name:"Empty"}]},"name",true,"",{"source:source":true})
        compare(rows.length,2)
        compare(rows[1].name,"Empty")
        compare(rows[1].tracks,0)
        compare(rows[1].expandKey,"source folder:e")
    }
    function test_mixed_native_folders_have_no_provider_wrapper() {
        const catalog = {folder_sources:[{id:"source",name:"Cloud"}],
            native_folders:[{source:"source",ref:"folder:e",name:"Favorites",parent:"",folder_icon:"folder.svg"}],
            playlists:[{id:-1,name:"Loose",source:"source",provider:"Cloud",separate:false,mixed:false},
                {id:-2,name:"Nested",source:"source",provider:"Cloud",native_folder:"folder:e",separate:false,mixed:false}]}
        const rows = Tree.rows(catalog,"name",true,"",{"source folder:e":true})
        compare(rows.map(r=>r.name),["Favorites","Nested","Loose"])
        compare(rows[0].depth,0)
        compare(rows[1].depth,1)
    }
}
