import QtQuick
import "qrc:/qt/qml/app/chilpa/qml/Components"

// A record a source sells, as the plugin manager's purchases draw it: the
// library's card, which opens the record's downloads.
AlbumCard {
    property var record: ({})
    signal opened()
    purchaseMode: true
    title: record.title || ""
    subtitle: record.artist || ""
    artistDestination: record.artist && record.plugin ? ({target:"source",key:record.plugin + " " + (record.artist_ref ? "native:" + record.artist_ref : "matching-artist-v3:" + record.artist)}) : ({})
    year: record.year || ""
    art: record.image || ""
    available: record.downloadable !== false
    onClicked: opened()
}
