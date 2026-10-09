import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/sources/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
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
    SourceShelf { width: Metrics.confirmWidth; section: ({title:"Releases",error:"Unavailable",key:""}) }
    SourcePaging { about: ({plugin:"preview",next:"next"}) }
    SourceLibraryControls { source: "preview section:albums" }
    StartupSplash { width: Metrics.confirmWidth; height: Metrics.panesHeight }
    PluginValidation { width: Metrics.confirmWidth; phase: "passed" }
    PluginRow { width: Metrics.confirmWidth; plugin: ({name: "Catalog source", light: "yellow"}) }
    PluginSettingsForm {
        width: Metrics.confirmWidth
        document: ({form: {fields: [{key: "user", label: "Account", control: {type: "text"}}], actions: []}, values: {}})
    }
    PluginIntegration { width: Metrics.confirmWidth; plugin: ({id: "guest", tint: false}) }
    PluginDetails { manager: preview }
    PluginRepositories { manager: preview }
    PluginManager { id: preview; width: Metrics.confirmWidth; height: Metrics.panesHeight; page: sample }
}
