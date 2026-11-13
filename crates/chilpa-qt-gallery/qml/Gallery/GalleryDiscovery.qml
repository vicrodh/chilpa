import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/discovery/qml/Components"
import "qrc:/qt/qml/app/chilpa/discovery/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"

TitledFlow {
    id: section
    title: "Discovery"
    LibraryDestinations {}
    ArtistContext { width: Metrics.settingsMaxWidth; info: ({title:"Artist",genres:["Jazz","Soul Jazz"],compositions:8}) }
    DiscoveryEntry { width: Metrics.discoveryEntryWidth; entry: ({kind:"genre",name:"Jazz"}); detail:"Explore styles in your library" }
    DiscoveryIndex {
        width: Metrics.discoveryEntryWidth * 2; height: Metrics.discoveryEntryHeight * 2 // derived: two preview cards across and two rows tall
        entries: [{kind:"genre",name:"Jazz",styles:4},{kind:"composer",name:"Composer",albums:3}]
    }
    DiscoveryTracks { width: Metrics.discoveryEntryWidth * 2; height: Metrics.queueMin } // derived: the same two-card preview width
}
