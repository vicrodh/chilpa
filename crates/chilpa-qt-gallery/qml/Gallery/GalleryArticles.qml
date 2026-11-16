import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/pages/qml/Components"
import "qrc:/qt/qml/app/chilpa/pages/qml/Patterns"

TitledFlow {
    id:section
    title:"Articles"
    QtObject {
        id:previewReader
        readonly property int imageRevision:0
        function image(url,revision) { return "" }
        function watchImage(url) { return 0 }
        function releaseImage(ticket) {}
    }
    QtObject { id:previewViewport;property real contentY:0;readonly property real height:Metrics.pageHero*3 } // derived: a three-frame preview viewport
    readonly property var example:({layout:"article",title:"A room for music",article:{id:"preview",authors:["Author"],published_at:"2026-11-16",summary:"A native reading view with plain text and images.",blocks:[
        {type:"heading",level:2,text:"The space between notes"},
        {type:"paragraph",text:"A performance captures the instruments and the space around them."},
        {type:"quote",text:"Leave room for silence.",attribution:"An interview with the artist"},
        {type:"list",ordered:true,items:["The complete performance","The live recording"]},
        {type:"image",url:"https://example.test/preview.jpg",width:1200,height:700,caption:"An image placeholder"}
    ]}})
    ArticleProjection { id:projection;article:section.example.article }
    Column {
        width:Math.min(Metrics.articleWidth,section.width)
        spacing:Metrics.gap16
        ArticleHeader { width:parent.width;document:section.example;reader:previewReader;viewport:previewViewport }
        Repeater {
            model:projection.rows
            ArticleBlock { required property var modelData;width:parent.width;height:implicitHeight;entry:modelData;reader:previewReader;viewport:previewViewport;rowY:y }
        }
    }
    ArticleImage { width:Metrics.discoveryEntryWidth;height:Metrics.pageHero;reader:previewReader;viewport:previewViewport;description:"Unloaded article image" }
}
