.pragma library

// Where `item` sits inside `content`. I read each
// ancestor's geometry and transform first, so a binding that calls this
// follows any of them moving.
function pointIn(item, content) {
    for (let at = item; at && at !== content; at = at.parent) {
        at.x
        at.y
        at.width
        at.height
        at.scale
        at.rotation
        at.transformOrigin
    }
    return item.mapToItem(content, 0, 0)
}

function topIn(item, content) { return pointIn(item, content).y }
