import QtQuick
import "../Theme"

Rectangle {
    id: field
    property alias text: input.text
    property string placeholder: ""
    property int maximumLength: 16384
    readonly property alias editing: input.activeFocus
    implicitWidth:Metrics.selectWidth
    implicitHeight:Metrics.fieldHeightLarge * 2 // derived: two input lines
    radius:Metrics.radiusControl; color:Theme.color.fillField
    border.width:editing ? Metrics.focusRing : 0; border.color:Theme.color.accent
    Flickable {
        id: scroll
        anchors.fill:parent; anchors.margins:Metrics.fieldPadding
        contentWidth:width; contentHeight:input.contentHeight
        clip:true; boundsBehavior:Flickable.StopAtBounds
        TextEdit {
            id:input
            width:scroll.width
            textFormat:TextEdit.PlainText; wrapMode:TextEdit.Wrap
            activeFocusOnTab:field.enabled
            Keys.priority:Keys.BeforeItem
            Keys.onTabPressed: event => { nextItemInFocusChain(true).forceActiveFocus(Qt.TabFocusReason); event.accepted=true }
            Keys.onBacktabPressed: event => { nextItemInFocusChain(false).forceActiveFocus(Qt.BacktabFocusReason); event.accepted=true }
            font.pointSize:Type.body; color:Theme.color.textPrimary; selectionColor:Theme.color.rowSelected
            onTextChanged: if(text.length > field.maximumLength) remove(field.maximumLength,text.length)
            onCursorRectangleChanged: {
                if (cursorRectangle.y < scroll.contentY) scroll.contentY=cursorRectangle.y
                else if (cursorRectangle.y + cursorRectangle.height > scroll.contentY + scroll.height) scroll.contentY=cursorRectangle.y + cursorRectangle.height - scroll.height
            }
        }
        Text {
            width:parent.width
            visible:input.text === ""
            text:field.placeholder; textFormat:Text.PlainText; wrapMode:Text.Wrap
            color:Theme.color.textTertiary; font.pointSize:Type.body
        }
    }
}
