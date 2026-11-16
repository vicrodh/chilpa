import QtQuick
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/sources/qml/Screens"
import "Screens"
import "qrc:/qt/qml/app/chilpa/qml/Screens"

Loader {
    id: page
    required property string source
    property bool leaving: false
    readonly property var scrollView: item?.contentY !== undefined ? item : item?.scrollView || null
    property var presentation: ({})
    property var about: ({})
    function acceptPresentation() { if (!leaving) about = presentation }
    onPresentationChanged: acceptPresentation()
    onLeavingChanged: acceptPresentation()
    Component.onCompleted: acceptPresentation()
    onLoaded: { if (item.leaving !== undefined) item.leaving = Qt.binding(() => page.leaving) }
    sourceComponent: {
        if (["collection","mixed"].includes(about.layout)) return collection
        if (about.artist === true || / (native:artist:|matching-artist-v[23]:)/.test(source)) return artist
        return about.shelves !== undefined ? home : about.albumsOnly === false ? listing : albums
    }
    Component { id:collection; SourceCollectionScreen { source:page.source } }
    Component { id:artist; ArtistPage { key:page.source } }
    Component { id:home; SourceHome { key:page.source } }
    Component { id:listing; SourceListing { source:page.source } }
    Component { id:albums; AlbumsScreen { source:page.source } }
}
