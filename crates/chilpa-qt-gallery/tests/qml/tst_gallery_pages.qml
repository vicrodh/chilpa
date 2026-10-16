import QtQuick
import QtTest

// Every page of the Gallery loads: each type it names is found where the
// page looks for it. A page that does not load shows nothing, and says so
// only in a log nobody reads.
TestCase {
    name: "GalleryPages"

    readonly property var pages: [
        "../../qml/Gallery/Gallery.qml",
        "../../qml/Gallery/GalleryPrimitives.qml",
        "../../qml/Gallery/GalleryComponents.qml",
        "../../qml/Gallery/GalleryPatterns.qml",
        "../../../chilpa-qt-metadata/qml/Gallery/GalleryMetadata.qml",
        "../../../chilpa-qt-sources/qml/Gallery/GalleryPlugins.qml",
        "../../../chilpa-qt-controls/qml/Gallery/GalleryControls.qml",
        "../../../chilpa-qt-lyrics/qml/Gallery/GalleryLyrics.qml"
    ]

    function test_every_page_loads_data() {
        return pages.map(page => ({ tag: page.split("/").pop(), page: page }))
    }

    function test_every_page_loads(data) {
        const component = Qt.createComponent(Qt.resolvedUrl(data.page))
        compare(component.status, Component.Ready, component.errorString())
    }
}
