import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Item {
    id: picture
    required property var reader
    required property var viewport
    property string url: ""
    property string description: ""
    property real imageWidth: 0
    property real imageHeight: 0
    property bool inViewport: false
    property bool pageActive: false
    property bool pageVisible: true
    property double lease: 0
    readonly property bool needed: pageActive && inViewport && url !== ""
    readonly property real ratio: imageWidth>0 && imageHeight>0 ? imageWidth/imageHeight : 1
    // An unknown aspect reserves a stable frame; a portrait never exceeds the viewport.
    implicitHeight: Math.min(Math.max(Metrics.pageHero,width/ratio),Math.max(Metrics.pageHero,viewport.height*0.75))
    readonly property int decode: Math.min(2048,Math.ceil(width*Screen.devicePixelRatio/Metrics.artDecodeStep)*Metrics.artDecodeStep)
    readonly property url cachedImage: reader ? reader.image(url,reader.imageRevision) : ""
    function release() {
        const ticket=lease
        lease=0
        if(ticket && reader)reader.releaseImage(ticket)
    }
    function syncDemand() {
        if(!needed)release()
        else if(!lease && reader)lease=reader.watchImage(url)
    }
    onNeededChanged:syncDemand()
    onUrlChanged: { release();syncDemand() }
    Component.onCompleted:syncDemand()
    Component.onDestruction:release()
    Connections { target:picture.reader; function onImageRevisionChanged() { picture.syncDemand() } }
    Rectangle { anchors.fill:parent; color:Theme.color.surfaceRaised; visible:image.status!==Image.Ready }
    Image {
        id:image
        objectName:"articleImage"
        anchors.fill:parent
        source:picture.pageVisible && picture.inViewport && String(picture.cachedImage).startsWith("file:") ? "image://cover/"+picture.decode+"/"+encodeURIComponent(String(picture.cachedImage)) : ""
        sourceSize:Qt.size(picture.decode,picture.decode)
        asynchronous:true
        cache:false
        retainWhileLoading:false
        fillMode:Image.PreserveAspectFit
        Accessible.name:picture.description.slice(0,4096)
    }
}
