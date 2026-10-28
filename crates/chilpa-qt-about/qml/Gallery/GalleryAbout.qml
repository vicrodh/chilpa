import QtQuick
import app.chilpa.about
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Patterns"

// The About window, opened by its button, and its body at a card's width;
// the update panels; one card of the Welcome and the release notes as What's
// new lays them out, and the deck behind its buttons.
Item {
    id: gallery
    implicitWidth: column.implicitWidth + Metrics.contentX * 2 // derived: the samples and their margins
    implicitHeight: column.implicitHeight + Metrics.contentX * 2 // derived: the samples and their margins
    readonly property int sampleWide: 640

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentX
        spacing: Metrics.gap16
        DeveloperSettings { width: gallery.sampleWide; height: Metrics.panesHeight }
        DeveloperDiagnosticRow { width: gallery.sampleWide; saved: true; entry: ({label:"Renderer",saved:"auto",runtime:"vulkan",status:"info"}) }
        DeveloperDiagnosticSection { width: gallery.sampleWide; entry: ({title:"System",rows:[{label:"Architecture",runtime:"x86_64"}]}) }
        LogActionButton { text: "Copy all"; glyph: Icons.copy }
        LogControls { width: gallery.sampleWide }
        LogList { width: gallery.sampleWide; height: Metrics.panesHeight; rows: [{ts: 0, level: "warn", target: "audio", message: "Fixture session log"}] }
        LogUploaded { width: gallery.sampleWide; url: "https://paste.example/abc" }
        PillButton { text: "Developer logs"; onClicked: developerLogs.open() }
        PillButton { text: "Review report"; onClicked: developerReport.open() }
        PillButton { text: "Report an issue"; onClicked: reportIssue.open() }
        PillButton { text: "About"; onClicked: about.open() }
        AboutBody { width: gallery.sampleWide; doc: ({ version: "0.0.0", build: "2026-10-09 (405e25a54)", platform: "Linux (Qt)", links: [{ label: "Website", url: "https://chilpa.app" }] }) }
        UpdatesPanel { width: gallery.sampleWide }
        PluginUpdatesPanel { width: gallery.sampleWide }
        UpdatesIndicator {}
        UpdateNotice {}
        PillButton { text: "Welcome"; onClicked: Onboarding.command("welcome") }
        PillButton { text: "What's new"; onClicked: Onboarding.command("whatsNew") }
        DeckCard {
            width: gallery.sampleWide; height: gallery.sampleWide * 9 / 16 // derived: the art's 16:9
            eyebrow: "Getting started"
            model: ({ image: "your-music.png", title: "Your music", body: "Add the folders where your music lives.", actionLabel: "Add a folder", action: "libraryFolders" })
        }
        ReleaseNotes {
            width: gallery.sampleWide; height: gallery.sampleWide / 2 // derived: half as tall as wide
            doc: ({ hasBody: true, toc: [{ id: "fixes", label: "Fixes" }], blocks: [
                { kind: 0, level: 0, text: "Fixes", id: "fixes", url: "" }, { kind: 1, level: 1, text: "Playback keeps its place after a seek.", id: "", url: "" },
                { kind: 3, level: 0, text: "Full changelog", id: "", url: "https://github.com/vicrodh/chilpa" }] })
        }
    }
    DeveloperLogDialog { id: developerLogs }
    DeveloperReportDialog { id: developerReport }
    ReportIssueDialog { id: reportIssue; listenForRequests: false }
    OnboardingModal {}
    AboutModal { id: about }
}
