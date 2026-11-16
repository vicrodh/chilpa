import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.pages
import "../Components"
import "../Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

ListView {
    id:page
    objectName:"nativeArticlePage"
    required property string source
    property bool leaving:false
    property bool completed:false
    readonly property bool activePage:visible && !leaving
    readonly property real readingWidth:Math.max(1,Math.min(Metrics.articleWidth,width-Metrics.contentX*2)) // derived: both content margins stay free
    readonly property real readingX:(width-readingWidth)/2 // derived: center the reading column
    readonly property var document:decode(articleModel.document)
    function decode(value) { try{return JSON.parse(value || "{}")}catch(_){return {}} }
    function activate() {
        if(activePage)articleModel.open(source)
        else if(visible)articleModel.retire()
        else articleModel.close()
    }
    function showStart() {
        if(!activePage || !document.article)return
        forceLayout()
        // The header's text and image frame settle after the list's first layout.
        Qt.callLater(() => { if(page.activePage && page.document.article)page.contentY=page.headerItem.y-page.topMargin })
    }
    onDocumentChanged:if(completed && activePage && document.article)Qt.callLater(showStart)
    onSourceChanged:if(completed && activePage)articleModel.open(source)
    onActivePageChanged:if(completed)activate()
    onVisibleChanged:if(completed && !visible)articleModel.close()
    Component.onCompleted: { completed=true;activate() }
    Component.onDestruction:articleModel.close()
    NativeArticleModel { id:articleModel;objectName:"nativeArticleModel";active:page.activePage }
    // Idle polling reads only runtime/settings stamps, never the article body.
    PulseClock { slow:true;minimumInterval:articleModel.busy ? 100 : 1000;running:page.activePage && (articleModel.busy || !!page.document.article);onTriggered:articleModel.poll() }
    Connections { target:Sources;function onRevisionChanged(){if(page.activePage)articleModel.poll()} }
    ArticleProjection { id:projection;article:page.document.article || ({}) }
    model:projection.rows
    spacing:Metrics.gap16
    topMargin:Metrics.contentTop;bottomMargin:Metrics.contentBottom
    cacheBuffer:height*0.5
    clip:true
    boundsBehavior:Flickable.StopAtBounds
    ScrollBar.vertical:ViewportScrollBar {}
    header:Column {
        x:page.readingX;width:page.readingWidth
        spacing:Metrics.gap16
        ArticleHeader {
            width:parent.width
            document:page.document;reader:articleModel;viewport:page;rowY:parent.y+y
            pageActive:page.activePage;pageVisible:page.visible
        }
        Text {
            objectName:"articleCacheNote"
            width:parent.width;visible:page.document.offline === true || page.document.stale === true
            text:page.document.error || I18n.tr("Available offline",I18n.rev)
            textFormat:Text.PlainText;wrapMode:Text.Wrap;font.pointSize:Type.small;color:Theme.color.textSecondary
        }
        BusyIndicator { width:Metrics.button32;height:width;visible:articleModel.busy && !page.document.article;running:visible && page.activePage }
        Text { width:parent.width;visible:articleModel.error !== "";text:articleModel.error;textFormat:Text.PlainText;wrapMode:Text.Wrap;color:Theme.color.textSecondary }
        PillButton { visible:articleModel.error !== "";text:I18n.tr("Retry",I18n.rev);onClicked:articleModel.open(page.source) }
        Item { width:parent.width;height:Metrics.gap16;visible:!!page.document.article }
    }
    delegate:Item {
        id:row
        required property var modelData
        width:page.width;height:block.implicitHeight
        ArticleBlock {
            id:block
            x:page.readingX;width:page.readingWidth
            entry:row.modelData;reader:articleModel;viewport:page;rowY:row.y
            pageActive:page.activePage;pageVisible:page.visible
        }
    }
}
