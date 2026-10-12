import QtQuick
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/sources/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"

TitledFlow {
    title: "Plugins"
    MediaListeningPanel { width: Metrics.confirmWidth; height: Metrics.panesHeight }
    EpisodeMaterial { width: Metrics.confirmWidth; entry: ({description:"Episode notes"}) }
    ImportCheck { text: "Mixed playlist"; checked: true; width: Metrics.confirmWidth }
    ImportMatch { width: Metrics.confirmWidth; match: ({title:"Track",artist:"Artist",confidence:0.95}); sourceName: "Source"; destination: "Mixed"; qualityLabel: "FLAC" }
    MediaArtwork { width: Metrics.pageHero; height: width; station: true; title: "Station" }
    MediaCard { width: Metrics.pageHero; station: true; entry: ({title:"Station",subtitle:"Jazz · AAC 128",playable:true}) }
    MediaRow { width: Metrics.confirmWidth; entry: ({title:"Episode",description:"Notes",finite:true,playable:true,progress:0.3}) }
    MediaShelf { width: Metrics.confirmWidth; title: "Your shows"; entries: [{title:"Show",id:"show",kind:"folder"}] }
    MediaGrid { width: Metrics.confirmWidth; station: true; entries: [{id:"station",title:"Station",playable:true}] }
    MediaQuickView { width: Metrics.confirmWidth; entry: ({title:"Station",subtitle:"Country · Jazz",playable:true}) }
    MediaShowHeader { width: Metrics.confirmWidth; entry: ({title:"Show",subtitle:"Author",description:"A description of the show."}) }
    SourceDiscoveryControls { width: Metrics.confirmWidth; about: ({plugin:"preview",tab:"home",genres:[],shelves:[{id:"releases",title:"Releases",items:[]}]}) }
    AlbumInfoModal {}
    AlbumCreditRow { width: Metrics.confirmWidth; credit: ({title:"Song",artist:"Artist",performers:"Player, Guitar"}) }
    SourceRecordDetails { width: Metrics.confirmWidth }
    SourceAlbumLibraries { width: Metrics.confirmWidth; model: [{id:"preview",name:"Streaming library",enabled:false,icon:""}] }
    SourcesPopover {}
    SourcePlaylistEditor {}
    SourceFolderEditor {}
    SourcePlaylistRows { width: Metrics.confirmWidth; rows: [{title:"Song",artist:"Artist",member:"1"}] }
    SourcePageHeader { width: Metrics.confirmWidth; info: ({title:"Artist",description:"Biography"}) }
    ArtistHeader { width: Metrics.confirmWidth; info: ({title:"Artist",description:"Biography",roles:["Composer"],radio_id:-1}) }
    JumpBar { width: Metrics.confirmWidth; entries: [{id:"about",title:"About"},{id:"tracks",title:"Popular Tracks"}]; active: "about" }
    ArtistTracks { width: Metrics.confirmWidth; section: ({title:"Popular Tracks",role:"popular",key:""}) }
    ArtistNetwork { width: Metrics.networkPanel; height: Metrics.panesHeight; document: ({header:{title:"Artist"},shelves:[{role:"labels",items:[{title:"Label"}]},{role:"similar",items:[{title:"Another"}]}]}); network: ({origin:{person:false,begin:"1975",place:"Leyton"},relationships:{any:true,members:[{name:"Member"}]},similar:["Alike"]}) }
    SceneArtistCard { artist: ({title:"Scene artist",genres:["rock"],sources:[]}) }
    SceneAlphaRail { activeKeys: ["A", "B"] }
    SceneGenreFilter { width: Metrics.sceneGenreWidth; genres: ["rock", "post-punk", "shoegaze"]; selected: ["post-punk"] }
    SceneLoading { width: Metrics.confirmWidth; height: Metrics.panesHeight; progress: ({percentage:42,detail:"grunge"}) }
    ScenePanel { width: Metrics.confirmWidth; height: Metrics.panesHeight; artists: [{mbid:"preview",title:"Scene artist",genres:["post-punk"],sources:[]}]; selectedId: "preview" }
    ScenePanelRail { width: Metrics.scenePanelRailWidth; height: Metrics.panesHeight; artists: [{mbid:"preview",title:"Scene artist",sources:[]}] }
    ScenePanelAlbums { width: Metrics.confirmWidth; height: Metrics.panesHeight }
    SceneAlbumCard { album: ({title:"Scene album",artist:"Scene artist",year:"2026"}) }
    SourceShelf { width: Metrics.confirmWidth; section: ({title:"Releases",error:"Unavailable",key:""}) }
    SourceHomeSections { width: Metrics.confirmWidth }
    PillButton {
        text: "Customize shelves"
        onClicked: shelvesCustomizer.open()
        SectionsCustomizer { id: shelvesCustomizer; sections: [{ id: "a", title: "Recently played" }, { id: "b", title: "Most played", enabled: false, size: 12 }] }
    }
    SourcePaging { about: ({plugin:"preview",next:"next"}) }
    ArtistSources { source: "preview"; artist: "Maiden"; sources: [{ id: "preview", name: "Preview" }] }
    TrackStarsFilter { width: Metrics.sourcesPopoverWidth / 2; ratings: [5, 0] } // derived: a column of the popover
    SourceLibraryControls { source: "preview section:albums" }
    StartupSplash { width: Metrics.confirmWidth; height: Metrics.panesHeight }
    PluginValidation { width: Metrics.confirmWidth; phase: "passed" }
    PluginRow { width: Metrics.confirmWidth; plugin: ({name: "Catalog source", light: "yellow"}) }
    PluginSettingsForm {
        width: Metrics.confirmWidth
        document: ({status: {phase: "failed", label: "Session expired", description: "Reconnect your account to continue playback."},
            links: [{label: "Reconnect", url: "https://example.invalid", primary: true}], form: {fields: [], actions: []}, values: {}})
    }
    PluginPurchaseTrackRow { width: Metrics.confirmWidth; entry: ({title:"Purchased track",number:1,duration:213}); rowIndex: 0; formatId: "6" }
    PluginPurchaseDetail { width: Metrics.settingsMaxWidth; height: Metrics.panesHeight; selection: ({kind:"album",title:"Purchased album",artist:"Artist",tracks:[]}); document: ({destination_ready:true}) }
    PluginSettingsNavigation { width: Metrics.settingsNav }
    PluginDownloadsUnavailable { loading: false }
    PluginManagerDownloads { width: Metrics.settingsMaxWidth; height: Metrics.panesHeight; controller: preview }
    PluginDownloads {
        width: Metrics.settingsMaxWidth; height: Metrics.panesHeight
        plugin: ({name:"Download source",export_folder:"/Music"})
        document: ({signed_in:true,loaded:true,albums:[{id:"album",title:"Purchased album",artist:"Artist",formats:[{id:6,label:"FLAC"}]}],tracks:[]})
        purchaseCard: previewPurchaseCard
    }
    Component {
        id: previewPurchaseCard
        AlbumCard {
            property var record: ({})
            signal opened()
            purchaseMode: true
            title: record.title || ""
            subtitle: record.artist || ""
            art: record.image || ""
            onClicked: opened()
        }
    }
    PluginIntegration { width: Metrics.confirmWidth; plugin: ({id: "guest", tint: false}) }
    PluginDetails { manager: preview }
    PluginRepositories { manager: preview }
    PluginManager { id: preview; width: Metrics.confirmWidth; height: Metrics.panesHeight; page: sample }
}
