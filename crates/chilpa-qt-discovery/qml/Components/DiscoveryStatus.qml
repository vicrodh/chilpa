import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

Item {
    id: status
    property bool loading: false
    property string failure: ""
    property bool empty: false
    property string emptyText: ""
    signal retryRequested()
    clip: true
    PageSkeleton { objectName:"discoveryLoading"; visible:status.loading }
    Column {
        objectName:"discoveryMessage"
        anchors.centerIn:parent
        width:Math.min(parent.width,Metrics.settingsMaxWidth)
        spacing:Metrics.gap12
        visible:status.failure !== "" || !status.loading && status.empty
        Text {
            width:parent.width; horizontalAlignment:Text.AlignHCenter; wrapMode:Text.WordWrap; textFormat:Text.PlainText
            text:status.failure !== "" ? I18n.tr("Could not read the library",I18n.rev) : status.emptyText
            font.pointSize:Type.body; color:Theme.color.textSecondary
        }
        Text {
            width:parent.width; visible:status.failure !== ""; text:status.failure
            horizontalAlignment:Text.AlignHCenter; wrapMode:Text.WrapAtWordBoundaryOrAnywhere; textFormat:Text.PlainText
            font.pointSize:Type.meta; color:Theme.color.textSecondary
        }
        PillButton {
            objectName:"discoveryRetry"; anchors.horizontalCenter:parent.horizontalCenter
            visible:status.failure !== ""; text:I18n.tr("Retry",I18n.rev); onClicked:status.retryRequested()
        }
    }
}
