import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Screens"
import "../../qml/Components"

TestCase {
    name: "ArtistScene"; when: windowShown; visible: true; width: 1100; height: 800
    Component { id: maker; ArtistScene { width: 1100; height: 800 } }
    Component { id: albumMaker; SceneAlbumCard { plugin: "fixture" } }
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
        const more = findChild(albums, "sceneSectionMore")
        verify(more !== null && more.visible)
        more.clicked()
        compare(Sources.asked[Sources.asked.length - 1], ["section_more", "fixture native:artist:7|album", ""])
        panel.selectedPlugin = "second"
        compare(panel.pageKey, "second native:artist:8")
        compare(albums.sections.map(s => s.title), ["Other catalog"])
        scene.selectedArtistId = "b"
        compare(albums.noSource, true)
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
