// Registers a font shipped in the binary's resources and makes its family
// the application's default, before any QML is loaded: every Text then draws
// with it without naming it. cxx-qt-lib has no binding for QFontDatabase, and
// this is the one call the window needs from it.

#include <QtGui/QFont>
#include <QtGui/QCursor>
#include <QtGui/QFontDatabase>
#include <QtGui/QGuiApplication>
#include <QtCore/QChar>
#include <QtCore/QHash>
#include <QtCore/QResource>
#include <QtCore/QStandardPaths>

namespace {

// The family a font of the resources registers as, adding the file once: the
// main font and the Japanese fallback can be the same face, and a second
// registration would be a second face in the font database.
QString registeredFamily(const char *resource)
{
    static QHash<QString, QString> registered;
    const QString path = QString::fromUtf8(resource);
    if (const auto known = registered.constFind(path); known != registered.constEnd())
        return *known;
    // An uncompressed font of the resources is read where it lies in the
    // binary: loading it by name first copies the whole file to the heap,
    // 3.6 MB for the Japanese face.
    const QResource file(path);
    const int id = file.isValid() && file.compressionAlgorithm() == QResource::NoCompression
        ? QFontDatabase::addApplicationFontFromData(
              QByteArray::fromRawData(reinterpret_cast<const char *>(file.data()), qsizetype(file.size())))
        : QFontDatabase::addApplicationFont(path);
    const QStringList families = id < 0 ? QStringList() : QFontDatabase::applicationFontFamilies(id);
    if (families.isEmpty())
        return QString();
    registered.insert(path, families.first());
    return families.first();
}

} // namespace

extern "C" bool chilpa_use_font(const char *resource)
{
    const QString family = registeredFamily(resource);
    if (family.isEmpty())
        return false;
    QFont font = QGuiApplication::font();
    font.setFamilies({family});
    QGuiApplication::setFont(font);
    return true;
}

// Makes a font of the resources the one Qt falls back to for Japanese, under
// any main font. Without it Qt takes the first face fontconfig says covers
// the script, and a face that claims Japanese without carrying the glyphs
// draws every kana as an empty box.
extern "C" bool chilpa_use_japanese_fallback(const char *resource)
{
    const QString family = registeredFamily(resource);
    if (family.isEmpty())
        return false;
    for (const auto script : {QChar::Script_Han, QChar::Script_Hiragana, QChar::Script_Katakana})
        QFontDatabase::addApplicationFallbackFontFamily(script, family);
    return true;
}

extern "C" const char *chilpa_fixed_font_family()
{
    static const QByteArray family = QFontDatabase::systemFont(QFontDatabase::FixedFont).family().toUtf8();
    return family.constData();
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
