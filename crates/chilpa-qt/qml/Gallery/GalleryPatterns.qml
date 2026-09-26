import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Patterns"

GallerySection {
    id: section
    title: "Patterns"

    Sidebar { width: Metrics.sidebarWidth; height: Metrics.panesHeight }
    SidebarPlaylists { width: Metrics.sidebarWidth; height: Metrics.panesHeight }
    RightPanel { width: Metrics.rightPanel; height: Metrics.queueDefault + Metrics.queueMin }
    Row {
        spacing: Metrics.gridGap
        QueuePanel { width: Metrics.rightPanel; height: Metrics.queueMin }
        LyricsPanel { width: Metrics.rightPanel; height: Metrics.queueMin }
    }
    Column {
        width: section.width - Metrics.sidebarWidth - Metrics.gridGap
        spacing: Metrics.gap12
        TabBar { width: parent.width }
        PlayerBar { width: Math.min(parent.width, Metrics.playerMaxWidth) }
        PlayerLcd { width: parent.width / 2 } // derived: half the section
        QualityTooltip {}
        SearchModel { id: sample; text: "mora" }
        SearchFlyout { search: sample }
        SearchBox { width: Metrics.screenSearch; active: false }
        PlaybackOptions {}
    }
}
