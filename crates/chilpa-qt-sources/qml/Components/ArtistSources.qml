import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Where an artist's page comes from, at the top right of its header: the
// library (Chilpa) or one of the installed sources with artist pages, each
// opening the same artist by name. On a source, its catalog or what the
// listener keeps of the artist there. Hidden when no source has artist
// pages: the library alone has nothing to switch to.
Row {
    id: switches
    // The plugin the page is of, or "" for the library.
    property string source
    property string artist
    property bool inLibrary: false
    // The installed sources with artist pages: [{ id, name }].
    property var sources: []
    signal libraryChosen(bool on)
    spacing: Metrics.gap8
    visible: sources.length > 0 && artist !== ""

    SegmentedControl {
        objectName: "artistSource"
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
        visible: switches.source !== ""
        options: [{ text: I18n.tr("From catalog", I18n.rev) }, { text: I18n.tr("In library", I18n.rev) }]
        current: switches.inLibrary ? 1 : 0
        onActivated: (index) => switches.libraryChosen(index === 1)
    }
}
