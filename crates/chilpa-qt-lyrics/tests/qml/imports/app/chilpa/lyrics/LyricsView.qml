pragma Singleton
import QtQuick
QtObject {
    // The facade's answer, number for number; its own tests hold it there.
    function dim(mode, synced, position, current) {
        if (!synced)
            return 0.85
        if (mode === "off" || current < 0 || position === current)
            return 1
        if (mode === "soft")
            return 0.6
        const ladder = position < current ? [0.5, 0.35, 0.25, 0.18] : [0.4, 0.2, 0.08, 0.03]
        return ladder[Math.min(Math.abs(position - current), ladder.length) - 1]
    }
}
