pragma Singleton
import QtQuick
QtObject {
    property var requests: []
    property var revalidations: []
    property bool allowWeb: false
    function coverOverride(id,rev) { return "" }
    function requestKey(key, query) { requests = requests.concat([[key,JSON.parse(query)]]); return key + ":filtered" }
    function sameNavigation(key, navigation, rev) { return key === navigation || key.startsWith(navigation + ":") }
    function revalidate(key, manual) { revalidations = revalidations.concat([[key,manual]]) }
    function webAllowed(plugin, initial, address, rev) { return allowWeb }
}
