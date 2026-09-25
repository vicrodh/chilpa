import QtQuick
import "../Theme"
import "../Patterns"

GallerySection {
    id: section
    title: "Patterns"

    Sidebar { width: Metrics.sidebarWidth; height: Metrics.panesHeight }
    Column {
        width: section.width - Metrics.sidebarWidth - Metrics.gridGap
        spacing: Metrics.gap12
        TabBar { width: parent.width }
        PlayerBar { width: Math.min(parent.width, Metrics.playerMaxWidth) }
        PlayerLcd { width: parent.width / 2 } // derived: half the section
        QualityTooltip {}
    }
}
