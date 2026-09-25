import QtQuick
import com.blitzfc.chilpa
import "../Theme"

// A destination whose screen is not built yet. It says so plainly instead
// of showing a page that looks finished and does nothing.
Item {
    Text {
        anchors.centerIn: parent
        text: I18n.tr("This screen is not built yet.", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }
}
