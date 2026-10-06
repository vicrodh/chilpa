.pragma library

// I style each anchor separately; Text.linkColor would recolor every artist.
function render(markup, hovered, normal, active) {
    return String(markup || "").replace(/<a\b([^>]*)>/gi, function(tag, attributes) {
        const href = attributes.match(/\bhref\s*=\s*(["'])(.*?)\1/i)
        const color = href && hovered && href[2] === hovered ? active : normal
        const clean = attributes.replace(/\sstyle\s*=\s*(["']).*?\1/gi, "")
        return '<a' + clean + ' style="text-decoration: none; color: ' + color + ';">'
    })
}
