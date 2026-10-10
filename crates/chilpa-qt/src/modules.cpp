// Brings in the QML modules of the other shell crates. cxx-qt links a
// library crate's module registration only into that crate's own binaries,
// so the window registers them itself, before any QML is loaded.

#include <QtCore/QtPlugin>

extern const QStaticPlugin qt_static_plugin_app_chilpa_kit_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_ambient_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_mini_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_immersive_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_waveform_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_graphics_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_link_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_artwork_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_about_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_stats_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_sources_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_controls_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_folders_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_gallery_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_lyrics_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_metadata_plugin();

extern "C" void chilpa_import_modules()
{
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_kit_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_ambient_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_mini_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_immersive_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_waveform_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_graphics_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_link_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_artwork_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_about_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_stats_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_sources_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_controls_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_folders_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_gallery_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_lyrics_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_metadata_plugin());
}
