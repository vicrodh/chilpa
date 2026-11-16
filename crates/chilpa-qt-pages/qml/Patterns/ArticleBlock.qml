import QtQuick
import "../Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Item {
    id:block
    required property var entry
    required property var reader
    required property var viewport
    required property real rowY
    property bool pageActive:false
    property bool pageVisible:true
    readonly property bool imageBlock:entry.type === "image"
    readonly property bool indented:["quote","listItem"].includes(entry.type)
    readonly property real indent:Metrics.gap24+Metrics.gap16 // derived: space for three list digits and their trailing gap
    implicitHeight:imageBlock ? picture.implicitHeight : text.implicitHeight
    ArticleImage {
        id:picture
        width:parent.width
        height:implicitHeight
        visible:block.imageBlock
        reader:block.reader;viewport:block.viewport
        url:block.imageBlock ? block.entry.url || "" : ""
        description:block.entry.alt || ""
        imageWidth:block.entry.width || 0;imageHeight:block.entry.height || 0
        pageActive:block.pageActive && visible;pageVisible:block.pageVisible
        inViewport:block.rowY+height>viewport.contentY && block.rowY<viewport.contentY+viewport.height
    }
    Rectangle {
        visible:block.entry.type === "quote"
        width:Metrics.gap2;height:text.implicitHeight
        color:Theme.color.textTertiary
    }
    Text {
        visible:block.entry.type === "listItem" && block.entry.continuation !== true
        width:block.indent
        text:block.entry.marker || ""
        textFormat:Text.PlainText;font.pointSize:Type.reading;color:Theme.color.textPrimary
    }
    Text {
        id:text
        objectName:"articleBlockText"
        visible:!block.imageBlock
        x:block.indented ? block.indent : 0
        width:parent.width-x
        text:block.entry.text || ""
        textFormat:Text.PlainText
        wrapMode:Text.Wrap
        font.pointSize:block.entry.type === "heading" ? Type.section : ["caption","attribution"].includes(block.entry.type) ? Type.small : Type.reading
        font.weight:block.entry.type === "heading" ? Type.bold : block.entry.type === "summary" ? Type.medium : Type.regular
        font.italic:block.entry.type === "quote"
        color:["caption","attribution"].includes(block.entry.type) ? Theme.color.textSecondary : Theme.color.textPrimary
        lineHeight:1.4
        lineHeightMode:Text.ProportionalHeight
    }
}
