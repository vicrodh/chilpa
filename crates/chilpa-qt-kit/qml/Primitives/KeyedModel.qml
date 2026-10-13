import QtQuick

// I patch descriptors by identity. A cover or a late module changes its
// row while every unrelated delegate and scroll position stays alive.
ListModel {
    id: rows
    property var items: []
    property string keyField: "id"
    dynamicRoles: true
    onItemsChanged: Qt.callLater(reconcile)
    Component.onCompleted: reconcile()
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
