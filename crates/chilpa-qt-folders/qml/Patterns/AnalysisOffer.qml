import QtQuick
import app.chilpa
import app.chilpa.folders
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"

// When a folder joins the library, whether to analyze what it holds too.
// Reading tempo, key and loudness takes far longer than the scan, so it is
// asked, never assumed; it waits for the scan and runs in the background.
// Mounted once in the window, so a folder added from anywhere asks.
ConfirmDialog {
    id: offer
    objectName: "analysisOffer"
    // The folder it asks about: only what it holds is analyzed.
    property string folder
    title: I18n.tr("Analyze the new music too?", I18n.rev)
    message: I18n.tr("Chilpa can also read the tempo, musical key and loudness of what this folder holds. It takes much longer than reading the folder: it starts when the scan is done, runs in the background, and can be stopped in Settings > Library > Analysis.", I18n.rev)
    confirmText: I18n.tr("Analyze", I18n.rev)
    cancelText: I18n.tr("Not now", I18n.rev)
    onConfirmed: LibraryFolders.analyzeAfterScan(folder)

    Connections {
        target: LibraryFolders
        function onAdded(path) {
            if (!LibraryFolders.offersAnalysis())
                return
            offer.folder = path
            offer.open()
        }
    }
}
