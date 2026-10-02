import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"

// What a source's listing says while it has nothing to show: that it is on
// its way, that it failed and can be asked again, or that it is empty.
Column {
    id: state

    property string source
    property var about: ({})

    spacing: Metrics.gap12

    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        text: state.about.error ? state.about.error : state.about.loading ? I18n.tr("Loading…", I18n.rev) : I18n.tr("Nothing here.", I18n.rev)
        font.pointSize: Type.small
        color: state.about.error ? Theme.color.statusNegative : Theme.color.textSecondary
    }
    PillButton {
        anchors.horizontalCenter: parent.horizontalCenter
        visible: !!state.about.error
        text: I18n.tr("Try again", I18n.rev)
        onClicked: Sources.command("retry_listing", state.source, "")
    }
}
