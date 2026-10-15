import QtQuick

// The visible row and its offset survive a reset of the same track list.
QtObject {
    required property var model
    required property var view
    property var place: null
    function identity() { return model ? model.kind + "\n" + model.albumId + "\n" + model.key : "" }
    function keep() {
        const row = view.indexAt(0, view.contentY)
        const item = view.itemAtIndex(row)
        place = item ? { list: identity(), row: row, into: view.contentY - item.y } : null
    }
    function restore() {
        const was = place
        place = null
        if (!was || was.list !== identity() || view.count === 0)
            return
        view.forceLayout()
        if (was.row >= view.count) {
            view.positionViewAtEnd()
            return
        }
        view.positionViewAtIndex(was.row, ListView.Beginning)
        view.contentY += was.into
        view.returnToBounds()
    }
}
