import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

QtObject {
    property bool devices: true
    readonly property var entries: [
        { screen: "home", glyph: Icons.home, text: I18n.tr("Home", I18n.rev) },
        { screen: "library", glyph: Icons.columns3, text: I18n.tr("Library", I18n.rev) },
        { screen: "artists", glyph: SourceGlyphs.musicArtist, family: SourceGlyphs.font, text: I18n.tr("Artists", I18n.rev) },
        { screen: "albums", glyph: Icons.disc, text: I18n.tr("Albums", I18n.rev) },
        { screen: "tracks", glyph: Icons.music, text: I18n.tr("Tracks", I18n.rev) },
        { screen: "genres", glyph: Icons.hierarchy2, text: I18n.tr("Genres", I18n.rev) },
        { screen: "composers", glyph: Icons.pencil, text: I18n.tr("Composers", I18n.rev) },
        { screen: "compositions", glyph: Icons.music, text: I18n.tr("Compositions", I18n.rev) },
        { screen: "folders", glyph: Icons.folder, text: I18n.tr("Folders", I18n.rev) },
        { screen: "openMedia", glyph: Icons.folderOpen, text: I18n.tr("Open media", I18n.rev) }
    ].concat(devices ? [{ screen: "deviceSync", glyph: Icons.deviceMobile, text: I18n.tr("Devices", I18n.rev) }] : [])
}
