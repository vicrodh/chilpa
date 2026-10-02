// Brings in the QML module of `chilpa-qt-sources`. cxx-qt links a library
// crate's module registration only into that crate's own binaries, so the
// window registers it itself, before any QML is loaded.

#include <QtCore/QtPlugin>

extern const QStaticPlugin qt_static_plugin_com_blitzfc_chilpa_sources_plugin();

extern "C" void chilpa_import_modules()
{
    qRegisterStaticPluginFunction(qt_static_plugin_com_blitzfc_chilpa_sources_plugin());
}
