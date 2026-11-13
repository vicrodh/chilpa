.pragma library

// Display units never become selection keys or query operands.
function display(field, value) {
    const text = String(value === undefined || value === null ? "" : value)
    if (field !== "bits" && field !== "rate") return text
    if (!/^\d+$/.test(text)) return text
    if (Number(text) <= 0) return ""
    if (field === "bits") return text + "-bit"
    if (field === "rate") {
        const hertz = Number(text)
        return hertz >= 1000000 ? String(hertz / 1000000) + "MHz" : String(hertz / 1000) + "kHz"
    }
    return text
}
