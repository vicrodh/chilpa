.pragma library

// A file dialog's URL as the path on disk, for every dialog in the window.
// The escapes are undone (`Album%20%5BFLAC%5D` is `Album [FLAC]`), and on
// Windows the drive comes without the slash before it: `file:///C:/Music`
// is `C:/Music`, which `/C:/Music` is not. Anything that is not a file URL
// is already a path and is left alone. `os` stands in for Qt.platform.os.
function fromUrl(url, os) {
    const text = String(url || "")
    if (!text.startsWith("file://"))
        return text
    let path = text.slice("file://".length)
    try {
        path = decodeURIComponent(path)
    } catch (malformed) {
        // A stray `%` that escapes nothing is part of the name.
    }
    const windows = (os === undefined ? Qt.platform.os : os) === "windows"
    return windows && /^\/[A-Za-z]:/.test(path) ? path.slice(1) : path
}
