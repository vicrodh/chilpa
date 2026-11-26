import QtQuick
import QtTest
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/LocalPaths.js" as LocalPaths

// A dialog's URL turns into the path that is on disk, on Unix and on
// Windows alike, whatever platform the test runs on.
TestCase {
    name: "LocalPaths"

    function test_a_unix_url_keeps_its_root_and_loses_its_escapes() {
        compare(LocalPaths.fromUrl("file:///home/me/Music", "linux"), "/home/me/Music")
        compare(LocalPaths.fromUrl("file:///mnt/A/Album%20%5BFLAC%5D/Metr%C3%B3nomo.flac", "linux"), "/mnt/A/Album [FLAC]/Metrónomo.flac")
        compare(LocalPaths.fromUrl("file:///C:/odd", "linux"), "/C:/odd", "a folder named like a drive is only a drive on Windows")
    }

    function test_a_windows_url_starts_at_its_drive() {
        compare(LocalPaths.fromUrl("file:///C:/Users/me/Music/Caf%C3%A9.flac", "windows"), "C:/Users/me/Music/Café.flac")
        compare(LocalPaths.fromUrl("file:///g:/x", "windows"), "g:/x")
    }

    function test_what_is_not_a_file_url_is_left_alone() {
        compare(LocalPaths.fromUrl("/home/me/100%20.flac", "linux"), "/home/me/100%20.flac")
        compare(LocalPaths.fromUrl("", "linux"), "")
        compare(LocalPaths.fromUrl(undefined, "linux"), "")
        compare(LocalPaths.fromUrl("file:///home/me/100%.flac", "linux"), "/home/me/100%.flac", "a stray percent stays")
    }

    function test_the_platform_is_the_window_s_own_when_not_given() {
        compare(LocalPaths.fromUrl("file:///home/me"), "/home/me")
        compare(LocalPaths.fromUrl("file:///D:/x"), Qt.platform.os === "windows" ? "D:/x" : "/D:/x")
    }
}
