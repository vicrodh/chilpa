// I report Qt's native facts on demand; the facade composes the diagnostics.
#include <QtCore/QSysInfo>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtGui/QGuiApplication>
#include <QtGui/QClipboard>
#ifndef _WIN32
#include <dlfcn.h>
#endif
#ifdef __GLIBC__
#include <gnu/libc-version.h>
#endif

namespace {
QString loadedVersion(const char *symbol)
{
#ifndef _WIN32
    const auto version = reinterpret_cast<const char *(*)()>(dlsym(RTLD_DEFAULT, symbol));
    return version ? QString::fromLatin1(version()) : QString();
#else
    (void)symbol;
    return {}; // I do not query Unix audio libraries on Windows.
#endif
}
}

extern "C" const char *chilpa_developer_native_facts()
{
    static QByteArray bytes;
    bytes = QJsonDocument(QJsonObject{
        {"system", QSysInfo::prettyProductName()},
        {"architecture", QSysInfo::currentCpuArchitecture()},
        {"distribution", QSysInfo::productType()},
        {"distributionVersion", QSysInfo::productVersion()},
        {"alsa", loadedVersion("snd_asoundlib_version")},
        {"pipewire", loadedVersion("pw_get_library_version")},
        {"pulse", loadedVersion("pa_get_library_version")},
#ifdef __GLIBC__
        {"libc", QString::fromLatin1(gnu_get_libc_version())},
#endif
        {"kernel", QSysInfo::kernelType() + " " + QSysInfo::kernelVersion()},
        {"qt", QString::fromLatin1(qVersion())},
        {"platform", QGuiApplication::platformName()}
    }).toJson(QJsonDocument::Compact);
    return bytes.constData();
}
extern "C" void chilpa_developer_clipboard(const char *text)
{
    if (auto *clipboard = QGuiApplication::clipboard()) clipboard->setText(QString::fromUtf8(text));
}
