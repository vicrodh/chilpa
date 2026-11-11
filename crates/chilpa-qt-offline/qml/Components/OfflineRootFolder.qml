import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// The offline folder every server inherits, as Music servers and the
// Offline music page both offer it.
FolderChoice {
    id: offline
    signal saved()
    title: I18n.tr("Offline folder", I18n.rev)
    description: I18n.tr("Downloads are grouped by server type. Each account has its own subfolder.", I18n.rev)
    defaultLabel: I18n.tr("App data folder", I18n.rev)
    defaultFolder: JSON.parse(Servers.setup || "{}").offlineRoot || ""
    folder: SettingsModel.kept("offline.folder") || ""
    onChosen: selected => {
        SettingsModel.keep("offline.folder", selected)
        folder = SettingsModel.kept("offline.folder") || ""
        error = folder === selected ? "" : I18n.tr("The settings could not be saved.", I18n.rev)
        Servers.refresh()
        offline.saved()
    }
}
