import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Where an artist's page comes from, at the top right of its header: the
// library (Chilpa) or one of the installed sources with artist pages, each
// opening the same artist by name. Under it, on a source, its catalog or
// what the listener keeps of the artist there, with how much that is; it
// takes the first line when there is no source to switch to.
Column {
    id: switches
    // The plugin the page is of, or "" for the library.
    property string source
    property string artist
    property bool inLibrary: false
    // How many tracks and records the listener keeps of the artist there.
    property int kept: 0
    // The installed sources with artist pages: [{ id, name }].
    property var sources: []
    signal libraryChosen(bool on)
    spacing: Metrics.gap8
    visible: artist !== "" && (sources.length > 0 || source !== "")

    SegmentedControl {
        objectName: "artistSource"
        anchors.right: parent.right
        visible: switches.sources.length > 0
        options: [{ text: "Chilpa" }].concat(switches.sources.map(s => ({ text: s.name })))
        current: switches.source === "" ? 0 : 1 + switches.sources.findIndex(s => s.id === switches.source)
        onActivated: (index) => {
            if (index === current)
                return
            if (index === 0)
                Navigation.openArtist(switches.artist)
            else
                Navigation.goKey("source", switches.sources[index - 1].id + " matching-artist-v3:" + switches.artist)
        }
    }
    SegmentedControl {
        objectName: "artistShelf"
        anchors.right: parent.right
        visible: switches.source !== ""
        options: [{ text: I18n.tr("From catalog", I18n.rev) }, { text: I18n.tr("In library", I18n.rev), count: switches.kept }]
        current: switches.inLibrary ? 1 : 0
        onActivated: (index) => switches.libraryChosen(index === 1)
    }
}
