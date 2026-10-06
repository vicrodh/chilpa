// Brings in the QML modules of the other shell crates. cxx-qt links a
// library crate's module registration only into that crate's own binaries,
// so the window registers them itself, before any QML is loaded.

#include <QtCore/QtPlugin>

extern const QStaticPlugin qt_static_plugin_com_blitzfc_chilpa_kit_plugin();
extern const QStaticPlugin qt_static_plugin_com_blitzfc_chilpa_sources_plugin();
extern const QStaticPlugin qt_static_plugin_com_blitzfc_chilpa_controls_plugin();
extern const QStaticPlugin qt_static_plugin_com_blitzfc_chilpa_gallery_plugin();
extern const QStaticPlugin qt_static_plugin_com_blitzfc_chilpa_lyrics_plugin();
extern const QStaticPlugin qt_static_plugin_com_blitzfc_chilpa_metadata_plugin();

extern "C" void chilpa_import_modules()
{
    qRegisterStaticPluginFunction(qt_static_plugin_com_blitzfc_chilpa_kit_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_com_blitzfc_chilpa_sources_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_com_blitzfc_chilpa_controls_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_com_blitzfc_chilpa_gallery_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_com_blitzfc_chilpa_lyrics_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_com_blitzfc_chilpa_metadata_plugin());
}
