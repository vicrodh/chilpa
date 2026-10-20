.pragma library

// Where `item` sits down `content`, a flickable's content item. I read each
// ancestor's geometry and transform first, so a binding that calls this
// follows any of them moving.
function topIn(item, content) {
    for (let at = item; at && at !== content; at = at.parent) {
        at.x
        at.y
        at.width
        at.height
        at.scale
        at.rotation
        at.transformOrigin
    }
    return item.mapToItem(content, 0, 0).y
}
