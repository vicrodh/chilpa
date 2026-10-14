// Registers a font shipped in the binary's resources and makes its family
// the application's default, before any QML is loaded: every Text then draws
// with it without naming it. cxx-qt-lib has no binding for QFontDatabase, and
// this is the one call the window needs from it.

#include <QtGui/QFont>
#include <QtGui/QCursor>
#include <QtGui/QFontDatabase>
#include <QtGui/QGuiApplication>
#include <QtCore/QStandardPaths>

extern "C" bool chilpa_use_font(const char *resource)
{
    const int id = QFontDatabase::addApplicationFont(QString::fromUtf8(resource));
    if (id < 0)
        return false;
    const QStringList families = QFontDatabase::applicationFontFamilies(id);
    if (families.isEmpty())
        return false;
    QFont font = QGuiApplication::font();
    font.setFamilies({families.first()});
    QGuiApplication::setFont(font);
    return true;
}

// Whether the system has a desktop entry by that name. Naming the window
// after one that is not installed -- a build run from the tree -- only gets
// a refusal from the desktop portal on every start; the platform knows where
// entries live, so this asks it instead of guessing the folders.
extern "C" bool chilpa_has_desktop_entry(const char *name)
{
    const QString file = QString::fromUtf8(name) + QStringLiteral(".desktop");
    return !QStandardPaths::locate(QStandardPaths::ApplicationsLocation, file).isEmpty();
}

extern "C" void chilpa_queue_cursor(bool busy)
{
    if (busy)
        QGuiApplication::setOverrideCursor(QCursor(Qt::BusyCursor));
    else
        QGuiApplication::restoreOverrideCursor();
}
