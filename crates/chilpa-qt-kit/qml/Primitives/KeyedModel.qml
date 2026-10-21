import QtQuick

// I patch descriptors by identity. A cover or a late module changes its
// row while every unrelated delegate and scroll position stays alive.
ListModel {
    id: rows
    property var items: []
    property string keyField: "id"
    dynamicRoles: true
    // A deferred reconcile must not outlive the model: Qt.callLater still
    // ran it after a Loader dropped the model in the same tick, and get()
    // on the dead ListModel crashed the window. The guard is plain JS, so
    // it can still be read once the model is gone.
    readonly property var guard: ({ alive: true })
    Component.onDestruction: guard.alive = false
    onItemsChanged: Qt.callLater(rows.later, guard)
    Component.onCompleted: reconcile()
    function later(guard) { if (guard.alive) reconcile() }
    function reconcile() {
        for (let index = 0; index < items.length; index++) {
            const item = items[index]
            const key = String(item[keyField] ?? index)
            const signature = JSON.stringify(item)
            let found = index
            while (found < count && get(found).key !== key) found++
            if (found === count)
                insert(index, { key: key, entry: item, signature: signature })
            else {
                if (found !== index) move(found, index, 1)
                if (get(index).signature !== signature) {
                    setProperty(index, "entry", item)
                    setProperty(index, "signature", signature)
                }
            }
        }
        while (count > items.length) remove(count - 1)
    }
}
