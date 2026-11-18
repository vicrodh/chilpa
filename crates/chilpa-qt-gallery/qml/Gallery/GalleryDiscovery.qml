import "qrc:/qt/qml/app/chilpa/pages/qml/Components"
import "qrc:/qt/qml/app/chilpa/pages/qml/Patterns"
import QtQuick
import app.chilpa
import app.chilpa.pages
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
    PillButton { text:"Genre images"; onClicked:artwork.open() }
    GenreArtworkEditor { id:artwork; key:'{"kind":"genre","name":"Jazz"}' }
    Loader { id:browser; active:false; sourceComponent:AlbumGenreBrowser { albumId:1; onFinished:browser.active = false } }
    LibraryDestinations {}
    SidebarEntryChoices { width:Metrics.settingsMaxWidth; value:"home,-library,artists" }
    QtObject { id:sampleNavigation; property var expandedSources:({}); property var sourceView:({}) }
    SourceNavigationTree { width:Metrics.sidebarWidth; pluginId:"fixture"; controller:sampleNavigation; entries:[{id:"collection",title:"Collection",children:[{id:"all",title:"All"}]}] }
    CollectionControls { width:Metrics.settingsMaxWidth; source:""; about:({filters:{},filter_controls:[{key:"search",type:"search",label:"Search collection"},{key:"hidden",type:"toggle",label:"Hide purchases"}]}) }
    SourceEntityHeader { width:Metrics.settingsMaxWidth; currentPage:true; info:({kind:"award",title:"Award",image_fallback:"award"}) }
    NativeCollectionModel { id:collectionWindow; active:false }
    CollectionActions { window:collectionWindow; about:({}); activePage:false }
    AlbumGridRow {
        first:0; span:3; revision:0; group:""
        owner:({visible:true,width:Metrics.settingsMaxWidth,cardWidth:CardLayout.cardWidth,
            rowHeight:CardLayout.cardWidth+Metrics.cardTextGap+Metrics.cardTextHeight,
            visibleTop:0,viewport:null,shelf:false,list:false,technical:false,
            layoutWidth:Metrics.settingsMaxWidth,columns:3,scrollView:{height:Metrics.queueMin},
            model:{expanded:-1},viewportWidth:Metrics.settingsMaxWidth,viewportX:0})
        cardDelegate: Component {
            CollectionTile {
                required property int index
                width:CardLayout.cardWidth
                x:index*(width+Metrics.gridGap)
                plugin:"fixture"
                entry:({kind:["artist","label","award"][index],ref:"fixture:"+index,
                    title:["Artist","Label","Award"][index],image_fallback:["user-star","disc","award"][index]})
            }
        }
    }
    Column {
        width: Metrics.confirmWidth
        PillButton { text: "Resize preceding section"; onClicked: sectionPreview.expanded = !sectionPreview.expanded }
        Flickable {
            id: sectionPreview
            property bool expanded: false
            width: parent.width; height: Metrics.artistPortrait; contentHeight: sectionColumn.height; clip: true
            Column {
                id: sectionColumn
                width: parent.width; spacing: Metrics.blockGap
                Repeater {
                    id: previewSections
                    model: 5
                    Item {
                        required property int index
                        readonly property var section: ({id:String(index)})
                        width: sectionColumn.width; height: Metrics.artistPortrait * (index === 0 && sectionPreview.expanded ? 2 : 1) // derived: double the first section to demonstrate anchoring
                        Text { text: "Section " + (parent.index + 1); color: Theme.color.textPrimary }
                    }
                }
            }
            ArtistSectionPosition { viewport: sectionPreview; column: sectionColumn; blocks: previewSections }
        }
    }
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
    DiscoveryAlbums { width:Metrics.settingsMaxWidth; height:Metrics.queueMin }
    DiscoveryArtists { width:Metrics.settingsMaxWidth; height:Metrics.queueMin; cards: Component { ArtistCardGrid { artists:parent.artists } } }
    DiscoveryStatus { width:Metrics.settingsMaxWidth; height:Metrics.queueMin; loading:true }
    DiscoveryStatus { width:Metrics.settingsMaxWidth; height:Metrics.queueMin; failure:"A library read failed" }
    DiscoveryStatus { width:Metrics.settingsMaxWidth; height:Metrics.queueMin; empty:true; emptyText:"No albums in this selection" }
    GenreOverview { width:Metrics.settingsMaxWidth; height:Metrics.queueMin; name:"Jazz"; about:({summary:{albums:4,images:[]},items:[],total:0}) }
}
