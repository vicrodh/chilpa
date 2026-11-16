import QtQuick
import "../Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Column {
    id:header
    required property var document
    required property var reader
    required property var viewport
    property bool pageActive:false
    property bool pageVisible:true
    property real rowY:0
    readonly property var article:document.article || ({})
    spacing:Metrics.gap16
    function metadata() {
        const values=[]
        if(article.authors?.length)values.push(article.authors.join(", "))
        if(article.published_at){
            const date=new Date(article.published_at)
            values.push(isNaN(date.getTime()) ? article.published_at : Qt.formatDate(date,Locale.LongFormat))
        }
        return values.join(" \u00b7 ")
    }
    Text {
        width:parent.width
        text:header.document.title || ""
        textFormat:Text.PlainText;wrapMode:Text.Wrap
        font.pointSize:Type.title;font.weight:Type.bold;color:Theme.color.textPrimary
    }
    Text {
        width:parent.width
        visible:text !== ""
        text:header.metadata()
        textFormat:Text.PlainText;wrapMode:Text.Wrap
        font.pointSize:Type.meta;color:Theme.color.textSecondary
    }
    ArticleImage {
        objectName:"articleCover"
        width:parent.width;height:visible ? implicitHeight : 0
        visible:!!header.article.image
        reader:header.reader;viewport:header.viewport
        url:header.article.image || ""
        imageWidth:header.article.image_width || 0;imageHeight:header.article.image_height || 0
        pageActive:header.pageActive && visible;pageVisible:header.pageVisible
        inViewport:header.rowY+y+height>viewport.contentY && header.rowY+y<viewport.contentY+viewport.height
    }
}
