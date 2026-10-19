import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// One option in a box: its name and what it does on the left, its control
// on the right, as a row of Settings is drawn.
Item {
    id: row

    property string title
    property string description
    default property alias control: holder.data

    implicitHeight: Math.max(Metrics.settingsRow, texts.implicitHeight + Metrics.gap12)

    SettingText {
        id: texts
        anchors.left: parent.left
        anchors.right: holder.left
        anchors.rightMargin: Metrics.gap16
        anchors.verticalCenter: parent.verticalCenter
        title: row.title
        description: row.description
    }
    Item {
        id: holder
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: childrenRect.width
        height: childrenRect.height
    }
}
