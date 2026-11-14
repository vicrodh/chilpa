import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

ScreenHead {
    id: head
    property alias text: find.text
    property alias placeholder: find.placeholder
    property string order: "name"
    property var orders: [{value:"name",text:I18n.tr("Name", I18n.rev)}, {value:"albums",text:I18n.tr("Most albums", I18n.rev),fixed:true}]
    signal orderPicked(string value)
    flexible: find
    SearchField {
        id: find
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(0, Math.min(Metrics.screenSearch, head.flexibleRoom))
        onEscaped: text = ""
    }
    SortButton {
        objectName: "collectionSort"
        anchors.verticalCenter: parent.verticalCenter
        value: head.order; options: head.orders
        onPicked: value => head.orderPicked(value)
    }
}
