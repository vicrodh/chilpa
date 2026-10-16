import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Screens"
import "../../qml/Components"
import "../../qml/Patterns"

TestCase {
    name: "ArtistScene"; when: windowShown; visible: true; width: 1100; height: 800
    Component { id: maker; ArtistScene { width: 1100; height: 800 } }
    Component { id: albumMaker; SceneAlbumCard { plugin: "fixture" } }
    Component { id: cardMaker; SceneArtistCard {} }
    function init() {
        Sources.sceneDocument = {state:"ready",includeSources:true,sceneLabel:"Seattle scene",artistName:"Source artist",genreSummary:"grunge / post-punk",
            artists:[
                {mbid:"a",title:"Cedar",genres:["grunge","post-punk"],sources:[{plugin:"fixture",name:"Fixture",ref:"artist:7"},{plugin:"second",name:"Second",ref:"artist:8"}]},
                {mbid:"b",title:"Briar",genres:["post-punk"],sources:[]},
                {mbid:"c",title:"Azure",genres:["shoegaze"],sources:[]}
            ],hasMore:true}
        Library.remoteRev++
    }
    function test_scene_filter_and_group_order() {
        const scene = createTemporaryObject(maker, this)
        waitForRendering(scene)
        compare(findChild(scene, "sceneArtistTitle").text, "Cedar", "the first visible card reads the source row's title")
        compare(scene.filteredArtists.map(a => a.title), ["Cedar", "Briar", "Azure"])
        scene.activeGenres = ["grunge", "post-punk"]
        compare(scene.filteredArtists.map(a => a.title), ["Cedar"], "genres use AND")
        scene.activeGenres = []
        scene.searchQuery = "  briar  "
        compare(scene.filteredArtists.map(a => a.title), ["Briar"], "text search trims the query")
        scene.searchQuery = ""
        scene.groupingEnabled = true
        compare(scene.groups.map(g => g.key), ["A", "B", "C"])
        scene.groupingEnabled = false
        compare(scene.filteredArtists[0].title, "Cedar", "the default keeps backend score order")
        scene.openArtist(scene.filteredArtists[0])
        compare(Navigation.went, ["source", "fixture native:artist:7"])
        scene.openArtist(scene.filteredArtists[1])
        compare(Navigation.went, ["artist", "Briar"])
    }
    function test_side_panel_opens_the_selected_artists_release_sections() {
        Sources.pages = {"fixture native:artist:7": JSON.stringify({
            shelves:[
                {id:"latest",kind:"album",title:"Latest release",items:[{ref:"album:1",title:"Repeat"}]},
                {id:"album",key:"fixture native:artist:7|album",kind:"album",title:"Discography",more:"artist:7:album:2",items:[{ref:"album:1",title:"First",artist:"Cedar",year:"2024"},{ref:"album:2",title:"Second"}]},
                {id:"single",kind:"album",title:"EPs & Singles",items:[{ref:"album:3",title:"Third"}]}
            ]
        }), "second native:artist:8": JSON.stringify({shelves:[{id:"album",kind:"album",title:"Other catalog",items:[{ref:"album:9",title:"Another"}]}]})}
        const scene = createTemporaryObject(maker, this)
        waitForRendering(scene)
        scene.mode = "sidepanel"
        scene.selectedArtistId = "a"
        const panel = findChild(scene, "scenePanel")
        const albums = findChild(scene, "scenePanelAlbums")
        compare(panel.pageKey, "fixture native:artist:7")
        compare(albums.sections.map(s => s.title), ["Discography", "EPs & Singles"])
        compare(albums.entries.length, 5, "the first section also has a continuation")
        compare(albums.entries[2].continuation.key, "fixture native:artist:7|album")
        tryVerify(() => { const button = findChild(albums, "sceneSectionMore"); return button !== null && button.visible })
        const more = findChild(albums, "sceneSectionMore")
        more.clicked()
        compare(Sources.asked[Sources.asked.length - 1], ["section_more", "fixture native:artist:7|album", ""])
        panel.selectedPlugin = "second"
        compare(panel.pageKey, "second native:artist:8")
        compare(albums.sections.map(s => s.title), ["Other catalog"])
        scene.selectedArtistId = "b"
        compare(albums.noSource, true)
    }
    function test_local_releases_and_progress_keep_the_existing_artist_card() {
        const local = {mbid:"local",title:"Local Band",genres:["rock"],sources:[{local:true,name:"Chilpa",key:"Local Band"}]}
        Sources.sceneDocument = {state:"ready",includeSources:true,artists:[local],loadingMore:true}
        Library.remoteRev++
        const scene = createTemporaryObject(maker, this)
        waitForRendering(scene)
        const card = findChild(scene, "sceneArtistCard")
        verify(card !== null)
        Sources.sceneDocument = {state:"ready",includeSources:true,artists:[Object.assign({}, local, {sources:local.sources.concat([{plugin:"fixture",name:"Fixture",ref:"artist:7"}])})],loadingMore:true}
        Library.remoteRev++
        tryVerify(() => findChild(card, "sceneArtistSource_fixture") !== null)
        compare(findChild(scene, "sceneArtistCard"), card, "a late source keeps the existing card and portrait alive")
        scene.mode = "sidepanel"; scene.selectedArtistId = "local"
        const panel = findChild(scene, "scenePanel")
        compare(panel.source.local, true)
        verify(findChild(scene, "sceneLocalAlbums").visible)
        verify(!findChild(scene, "scenePanelAlbums").visible)
        panel.selectedPlugin = "fixture"
        compare(panel.pageKey, "fixture native:artist:7")
        verify(!findChild(scene, "sceneLocalAlbums").visible)
        verify(findChild(scene, "scenePanelAlbums").visible)
    }
    function test_without_plugin_matches_keeps_the_grid() {
        const scene = createTemporaryObject(maker, this)
        waitForRendering(scene)
        scene.mode = "sidepanel"
        Sources.sceneDocument = {state:"ready",includeSources:false,sceneLabel:"Seattle scene",
            artists:[{mbid:"b",title:"Briar",genres:["post-punk"],sources:[]}]}
        Library.remoteRev++
        tryCompare(scene, "canBrowse", false)
        compare(scene.mode, "grid")
    }
    function test_catalogs_keep_independent_results_errors_and_real_genres() {
        var many = []
        for (var i = 0; i < 94; ++i)
            many.push({mbid:"q" + i,title:"Artist " + i,genres:i % 2 ? ["art rock"] : ["glam rock"],
                sources:[{plugin:"catalog_a",ref:"artist:" + i}]})
        var few = []
        for (var j = 0; j < 5; ++j)
            few.push({mbid:"a" + j,title:"Limited artist " + j,genres:["soul"],
                sources:[{plugin:"catalog_c",ref:"artist:us:" + j}]})
        Sources.sceneDocument = {state:"ready",includeSources:true,sceneLabel:"United Kingdom",artistName:"David Bowie",
            genreSummary:"rock",artists:many.concat(few),sources:[
                {id:"catalog_a",name:"Catalog A",state:"ready",artists:many,hasMore:true},
                {id:"catalog_b",name:"Catalog B",state:"error",error:"Authentication required",artists:[],hasMore:false},
                {id:"catalog_c",name:"Catalog C",state:"ready",artists:few,hasMore:false}
            ]}
        Library.remoteRev++
        const scene = createTemporaryObject(maker, this)
        waitForRendering(scene)
        compare(findChild(scene, "sceneSourceTabs"), null)
        compare(scene.allArtists.length, 99)
        compare(scene.availableGenres.sort(), ["art rock", "glam rock", "soul"])
        compare(findChild(scene, "sceneGenreFilter").genres.sort(), ["art rock", "glam rock", "soul"])
        scene.activeGenres = ["glam rock"]
        compare(scene.filteredArtists.length, 47)
        scene.activeGenres = ["soul"]
        compare(scene.filteredArtists.length, 5)
        compare(scene.shownDoc.state, "ready", "one unavailable source must not hide the unified scene")
        verify(scene.shownDoc.error.includes("Catalog B: Authentication required"))
    }
    function test_each_source_button_opens_its_own_artist_without_following_the_default() {
        const card = createTemporaryObject(cardMaker, this, {artist:{mbid:"same",title:"Band",sources:[
            {local:true,name:"Chilpa",key:"Local Band"},{plugin:"first",name:"First",ref:"artist:7"},{plugin:"second",name:"Second",ref:"artist:8"}]}})
        let whole = 0
        card.activated.connect(() => whole++)
        card.sourceActivated.connect(source => source.local ? Navigation.openArtist(source.key) : Navigation.goKey("source", source.plugin + " native:" + source.ref))
        waitForRendering(card)
        mouseClick(findChild(card, "sceneArtistSource_second"))
        compare(Navigation.went, ["source", "second native:artist:8"])
        mouseClick(findChild(card, "sceneArtistSource_library"))
        compare(Navigation.went, ["artist", "Local Band"])
        compare(whole, 0)
    }
    function test_album_action_accepts_opaque_storefront_refs() {
        const card = createTemporaryObject(albumMaker, this)
        card.album = {ref:"album:us:123",title:"Storefront album"}
        compare(card.albumId, -7)
        card.album = {ref:"album:123",title:"Catalog album"}
        compare(card.albumId, -7)
        card.album = {ref:"album:us:",title:"Incomplete album"}
        compare(card.albumId, 0)
    }
}
