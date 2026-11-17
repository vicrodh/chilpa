import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The library's own destinations, in the order and with the visibility the
// facade keeps (Settings > Navigation). `layout` is its normalized string:
// ids by comma, a leading "-" on one that is off. Empty means every entry.
QtObject {
    property string layout: Prefs.sidebar
    readonly property var all: [
        { screen: "home", glyph: Icons.home, text: I18n.tr("Home", I18n.rev) },
        { screen: "library", glyph: Icons.columns3, text: I18n.tr("Library", I18n.rev) },
        { screen: "artists", glyph: SourceGlyphs.musicArtist, family: SourceGlyphs.font, text: I18n.tr("Artists", I18n.rev) },
        { screen: "albums", glyph: Icons.disc, text: I18n.tr("Albums", I18n.rev) },
        { screen: "tracks", glyph: Icons.music, text: I18n.tr("Tracks", I18n.rev) },
        { screen: "genres", glyph: SourceGlyphs.genresTree, family: SourceGlyphs.font, text: I18n.tr("Genres", I18n.rev) },
        { screen: "composers", glyph: SourceGlyphs.composers, family: SourceGlyphs.font, text: I18n.tr("Composers", I18n.rev) },
        { screen: "compositions", glyph: SourceGlyphs.compositions, family: SourceGlyphs.font, text: I18n.tr("Compositions", I18n.rev) },
        { screen: "folders", glyph: Icons.folder, text: I18n.tr("Folders", I18n.rev) },
        { screen: "openMedia", glyph: Icons.folderOpen, text: I18n.tr("Open media", I18n.rev) },
        { screen: "deviceSync", glyph: Icons.deviceMobile, text: I18n.tr("Devices", I18n.rev) }
    ]
    // Every entry once, in the layout's order, each with whether it is on.
    readonly property var choices: layout === "" ? all.map(entry => Object.assign({ on: true }, entry))
        : layout.split(",").map(id => ({ id: id.replace(/^-/, ""), on: !id.startsWith("-") }))
            .map(item => Object.assign({ on: item.on }, all.find(entry => entry.screen === item.id) || {}))
            .filter(entry => entry.screen !== undefined)
    readonly property var entries: choices.filter(entry => entry.on)
}
