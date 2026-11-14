import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/discovery/qml/Components"
import "qrc:/qt/qml/app/chilpa/discovery/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"

TitledFlow {
    id: section
    title: "Discovery"
    AlbumGenreLine { width:Metrics.settingsMaxWidth; document:({items:[{name:"Progressive Metal",key:'{"kind":"genre","name":"Progressive Metal"}'}]}) }
    AlbumCredits { width:Metrics.settingsMaxWidth; record:({artistInformation:'{"artist_html":"Artist"}',genreLinks:'{"items":[]}'}) }
    PillButton { text:"Album genres"; onClicked:{ browser.active = true; browser.item.open() } }
    Loader { id:browser; active:false; sourceComponent:AlbumGenreBrowser { albumId:1; onFinished:browser.active = false } }
    LibraryDestinations {}
    ArtistContext { width: Metrics.settingsMaxWidth; info: ({title:"Artist",genres:["Jazz","Soul Jazz"],compositions:8}) }
    DiscoveryEntry { width: Metrics.discoveryEntryWidth; entry: ({kind:"genre",name:"Jazz",art:{key:"jazz"}}); detail:"Explore styles in your library" }
    DiscoveryEntry { width: Metrics.sceneCardWidth; activeImages:false; entry: ({kind:"composer",name:"Composer"}); detail:"3 albums" }
    DiscoveryMosaic { width: Metrics.discoveryEntryWidth; height: Metrics.discoveryGenreHeight; label:"Jazz"; art:({key:"jazz"}) }
    DiscoveryActions { width:Metrics.settingsMaxWidth; selected:({kind:"genre",name:"Jazz"}); choices:["Overview","Albums","Artists","Tracks","Subgenres"]; overview:true }
    DiscoveryTools { width:Metrics.settingsMaxWidth; title:"Genres" }
    DiscoveryBreadcrumb { title:"Genres"; trail:["Rock","Metal"] }
    GenreSources { sources:[{key:"local:default",label:"Local library"},{key:"plugin:fixture",label:"Source"}] }
    ArtistCard { artist:({title:"Artist",sources:[]}); activeImages:false }
    CollectionHead { width:Metrics.settingsMaxWidth; title:"Artists" }
    ArtistPortrait { width:Metrics.artistPortrait; height:width; name:"Artist"; activeImages:false }
    DiscoveryIndex {
        width: Metrics.discoveryEntryWidth * 2; height: Metrics.discoveryGenreHeight * 2 // derived: two preview cards across and two rows tall
        entries: [{kind:"genre",name:"Jazz",albums:4},{kind:"genre",name:"Rock",albums:3}]
    }
    DiscoveryWorkTable {
        width: Metrics.settingsMaxWidth; height: Metrics.queueMin
        entries: [{kind:"work",name:"Suite",composers:["Composer"],albums:3,tracks:12}]
    }
    DiscoveryTracks { width: Metrics.discoveryEntryWidth * 2; height: Metrics.queueMin } // derived: the same two-card preview width
    GenreOverview { width:Metrics.settingsMaxWidth; height:Metrics.queueMin; name:"Jazz"; about:({summary:{albums:4,images:[]},items:[],total:0}) }
}
