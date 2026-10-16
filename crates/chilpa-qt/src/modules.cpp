// I register the shell modules and set the bundled icon before creating windows.

#include <QtCore/QtPlugin>

extern const QStaticPlugin qt_static_plugin_app_chilpa_kit_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_ambient_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_mini_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_dsp_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_immersive_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_waveform_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_graphics_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_link_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_playlists_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_wizard_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_artwork_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_about_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_stats_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_sources_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_controls_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_folders_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_gallery_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_lyrics_plugin();
extern const QStaticPlugin qt_static_plugin_app_chilpa_metadata_plugin();

extern "C" void chilpa_frame_fence_register_qml_type();
extern "C" void chilpa_install_application_icon();

extern "C" void chilpa_install_shell()
{
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_kit_plugin());
    chilpa_frame_fence_register_qml_type();
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_ambient_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_mini_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_dsp_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_immersive_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_waveform_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_graphics_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_link_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_playlists_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_wizard_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_artwork_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_about_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_stats_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_sources_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_controls_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_folders_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_gallery_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_lyrics_plugin());
    qRegisterStaticPluginFunction(qt_static_plugin_app_chilpa_metadata_plugin());
    chilpa_install_application_icon();
}
