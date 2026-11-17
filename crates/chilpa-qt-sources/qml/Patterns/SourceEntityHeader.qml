import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
// I retire the shared source header together with its route.
Loader {
    id: header
    property var info: ({})
    property bool currentPage: false
    property bool pageInset: false
    Binding on x { when:header.pageInset; value:Metrics.contentX }
    Binding on y { when:header.pageInset; value:Metrics.contentTop }
    Binding on width {
        when:header.pageInset
        value:header.parent ? Math.max(0,header.parent.width - Metrics.contentX * 2) : 0 // derived: page gutters
    }
    active: currentPage && info && typeof info === "object" && !Array.isArray(info) && typeof info.title === "string" && info.title !== ""
    visible: active
    sourceComponent: SourcePageHeader {
        width: parent?.width || 0
        info: header.info
        currentPage: header.currentPage
    }
}
