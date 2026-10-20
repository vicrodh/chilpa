import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Patterns"
import "../../../chilpa-qt/qml/Components"

TestCase {
    id: test
    name: "HomeCarousels"
    when: windowShown
    visible: true
    width: 900; height: 800
    function init() {
        Sources.pages = {"fixture native:home": JSON.stringify({shelves:[
            {id:"records",items:Array.from({length:24}, (_, index) => ({ref:"album:"+index}))},
            {id:"tracks",items:Array.from({length:8}, (_, index) => ({ref:"track:"+index}))}
        ]})}
        Ratings.favorites = ({}); ++Ratings.rev
        Playlists.recentRows = Array.from({length:24}, (_, index) => ({id:index+1,name:"Playlist "+index,mixed:true}))
    }
    Component {
        id: playlistShelf
        HomePlaylistShelf { width:test.width; entry:({title:"Recently listened playlists",size:0}) }
    }
    Component {
        id: albumShelf
        AlbumCarousel {
            width: test.width; title:"Records"
            model: AlbumGridModel { kind:"source"; key:"fixture native:home|records"; limit:24 }
        }
    }
    Component {
        id: sourceShelf
        SourceShelf { width:test.width; oneRow:true; section:({title:"Ranked albums",presentation:"ranked",key:"fixture native:home|records",size:0}) }
    }
    Component {
        id: sourceTrackShelf
        SourceShelf { width:test.width; oneRow:true; section:({title:"Songs",tracks:true,key:"fixture native:home|tracks",size:3}) }
    }
    Component {
        id: trackShelf
        TrackCarousel { width:test.width; title:"Tracks"; model:TrackListModel { kind:"source"; key:"fixture native:home|tracks"; limit:3 } }
    }
    Component {
        id: mediaShelf
        MediaShelf { width:test.width; title:"Stations"; station:true; shown:3
            entries:Array.from({length:24}, (_, index) => ({id:String(index),title:"Station "+index,playable:true})) }
    }
    function test_album_count_grows_horizontally_and_keeps_card_height() {
        const shelf = createTemporaryObject(albumShelf, test)
        const rail = findChild(shelf,"albumCarouselRail")
        tryVerify(() => rail.height > 0 && shelf.model.count === shelf.model.columns)
        const height = shelf.height
        verify(!rail.interactive)
        shelf.size = 24
        tryVerify(() => rail.contentWidth > rail.width && rail.interactive)
        compare(shelf.model.count, 24)
        compare(shelf.height, height)
        const next = findChild(shelf,"shelfForward")
        verify(next.visible && next.enabled)
        next.clicked()
        verify(rail.contentX > 0)
        shelf.size = 0
        tryVerify(() => !rail.interactive)
        compare(shelf.height, height)
    }
    function test_ranked_source_modules_follow_the_same_single_card_row() {
        const shelf = createTemporaryObject(sourceShelf, test)
        const rail = findChild(shelf,"sourceShelfRail")
        tryVerify(() => shelf.ready && rail.height > 0)
        const height = shelf.height
        shelf.section = Object.assign({},shelf.section,{size:24})
        tryVerify(() => rail.contentWidth > rail.width)
        compare(shelf.height,height)
        verify(findChild(shelf,"shelfForward").enabled)
    }
    function test_track_cards_keep_playback_artist_and_favorite_actions() {
        const shelf = createTemporaryObject(trackShelf, test)
        const rail = findChild(shelf,"trackCarouselRail")
        tryVerify(() => rail.itemAtIndex(0) !== null)
        const height = shelf.height
        shelf.model.limit = 8
        tryVerify(() => rail.contentWidth > rail.width)
        compare(shelf.height,height)
        const card = rail.itemAtIndex(0)
        mouseClick(card,card.width / 2,card.width / 2)
        compare(shelf.model.lastAction,"play"); compare(shelf.model.lastRow,0)
        const artist = findChild(card,"trackCarouselArtist")
        mouseClick(artist,artist.width / 2,artist.height / 2)
        compare(shelf.model.lastAction,"artist")
        mouseMove(card,card.width / 2,card.width / 2)
        tryVerify(() => findChild(card,"trackCarouselFavorite") !== null)
        const favorite = findChild(card,"trackCarouselFavorite")
        shelf.model.lastAction = ""
        mouseClick(favorite,favorite.width / 2,favorite.height / 2)
        verify(Ratings.favorite("track",1,Ratings.rev))
        compare(shelf.model.lastAction,"")
        shelf.model.setProperty(0,"available",false)
        mouseClick(card,card.width / 2,card.width / 2)
        compare(shelf.model.lastAction,"")
    }
    function test_source_song_modules_use_one_rail_and_their_header_pages_it() {
        const shelf = createTemporaryObject(sourceTrackShelf,test)
        tryVerify(() => findChild(shelf,"trackCarouselRail") !== null)
        const height = shelf.height
        verify(!findChild(shelf,"sourceShelfRail").visible)
        shelf.section = Object.assign({},shelf.section,{size:8})
        tryVerify(() => findChild(shelf,"shelfHeader").paged)
        compare(shelf.height,height)
        findChild(shelf,"shelfHeader").forward()
        verify(findChild(shelf,"trackCarouselRail").contentX > 0)
    }
    function test_media_count_adds_a_scrollable_row_without_wrapping() {
        const shelf = createTemporaryObject(mediaShelf,test)
        const rail = findChild(shelf,"mediaShelfRail")
        tryVerify(() => rail.height > 0)
        const height = shelf.height
        shelf.shown = 24
        tryVerify(() => rail.contentWidth > rail.width)
        compare(shelf.height,height)
    }
    function test_playlist_count_keeps_local_and_mixed_history_in_one_row() {
        const shelf = createTemporaryObject(playlistShelf,test)
        const rail = findChild(shelf,"homePlaylistRail")
        tryVerify(() => rail.height > 0 && shelf.shown)
        const height = shelf.height
        verify(!rail.interactive)
        shelf.entry = Object.assign({},shelf.entry,{size:24})
        tryVerify(() => rail.contentWidth > rail.width)
        compare(shelf.height,height)
        verify(findChild(shelf,"shelfForward").enabled)
    }
}
