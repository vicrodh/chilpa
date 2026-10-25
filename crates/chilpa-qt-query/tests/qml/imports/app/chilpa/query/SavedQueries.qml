pragma Singleton
import QtQuick
QtObject {
    property string catalog: '{"entries":[],"revision":0}'
    property int rev: 0
    property string savedError: ""
    property var savedEntries: []
    property int savedNextId: 1
    property int savedLoads: 0
    property int previewReads: 0
    property int appliedQueries: 0
    function publishSaved() {
        rev++
        catalog = JSON.stringify({entries:savedEntries.map(entry => ({id:entry.id, name:entry.name})), revision:rev})
    }
    function load() { savedLoads++; return savedError }
    function previews() {
        previewReads++
        return savedError !== "" ? JSON.stringify({error:savedError}) : JSON.stringify({entries:savedEntries})
    }
    function save(name, expression) {
        if (savedError !== "") return savedError
        name = name.trim()
        if (!name) return "Name required"
        if (savedEntries.some(e => e.name === name)) return "Duplicate name"
        savedEntries = savedEntries.concat([{id:"q" + savedNextId++, name:name, expression:expression}])
        publishSaved()
        return ""
    }
    function rename(id, name) {
        if (savedError !== "") return savedError
        savedEntries = savedEntries.map(e => e.id === id ? Object.assign({}, e, {name:name}) : e)
        publishSaved()
        return ""
    }
    function remove(id) {
        if (savedError !== "") return savedError
        savedEntries = savedEntries.filter(e => e.id !== id)
        publishSaved()
        return ""
    }
    function expression(id) {
        if (savedError !== "") return JSON.stringify({error:savedError})
        const entry = savedEntries.find(e => e.id === id)
        if (!entry) return JSON.stringify({error:"Missing query"})
        appliedQueries++
        return JSON.stringify({expression:entry.expression})
    }

}
