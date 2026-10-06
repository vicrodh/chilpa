import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.folders
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"

// How a scan is going, or what the last one found: a line of text over a
// thin bar that fills while it reads.
Column {
    spacing: Metrics.gap6
    visible: LibraryFolders.scanning || LibraryFolders.status !== ""

    Text {
        text: LibraryFolders.status
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
    }

    Rectangle {
        visible: LibraryFolders.scanning
        width: parent.width
        height: Metrics.progress
        radius: 1
        color: Theme.color.strokeHairline

        Rectangle {
            width: parent.width * LibraryFolders.progress
            height: parent.height
            radius: 1
            color: Theme.color.accent
        }
    }
}
