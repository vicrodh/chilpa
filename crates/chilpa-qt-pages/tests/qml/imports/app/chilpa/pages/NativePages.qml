pragma Singleton
import QtQuick
import app.chilpa.sources
QtObject {
    property var requests: []
    property var revalidations: []
    function presentation(key,rev) {
        const page=JSON.parse(Sources.page(key,rev) || "{}")
        return JSON.stringify({music:page.music === true,layout:page.layout,artist:page.artist === true,albumsOnly:page.albumsOnly,shelves:page.shelves === undefined ? undefined : []})
    }
    function coverOverride(id,rev) { return "" }
    function requestKey(key, query) { requests = requests.concat([[key,JSON.parse(query)]]); return key + ":filtered" }
    function sameNavigation(key, navigation, rev) { return key === navigation || key.startsWith(navigation + ":") }
    function revalidate(key, manual) { revalidations = revalidations.concat([[key,manual]]) }
}
