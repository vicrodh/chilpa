import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/TextLinks.js" as TextLinks
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"

Modal {
    id: dialog
    property double albumId: 0
    property url art
    property string summary
    readonly property var info: visible ? JSON.parse(AlbumActions.information(albumId, Library.contentRev + Library.remoteRev) || "{}") : ({})
    property int activeTab: 0
    width: Math.min(parent.width - Metrics.modalInsets, Metrics.settingsMaxWidth + Metrics.heroArt)
    height: Math.min(parent.height - Metrics.modalInsets, Metrics.heroArt * 3) // derived: cover, credits and header share a bounded reading area
    onOpened: activeTab = 0
    onActiveTabChanged: reading.contentItem.contentY = 0
    onAlbumIdChanged: close()
    function follow(reference) {
        const plugin = info.plugin || ""
        if (/^(album|playlist):[^:]+/.test(reference)) {
            const id = Sources.referenceCard(plugin, reference)
            if (id) { close(); Navigation.go("album", id) }
        } else if (/^(artist|label):[^:]+/.test(reference) && plugin) {
            close(); Navigation.goKey("source", plugin + " native:" + reference)
        }
    }
    Clipboard { id: clipboard; objectName: "albumClipboard" }
    Menu {
        id: externalMenu; objectName: "albumExternalMenu"
        property string url
        entries: dialog.info.link_actions || []
        onChosen: index => { if (index === 0) Qt.openUrlExternally(url); else clipboard.write(url) }
    }
    Column {
        anchors.fill: parent
        spacing: Metrics.gap16
        Item {
            width: parent.width; height: Math.max(title.height, dismiss.height)
            Column {
                id: title; width: parent.width - headerActions.width - Metrics.gap16; spacing: Metrics.gap4
                Text { width: parent.width; text: dialog.info.title || ""; textFormat: Text.PlainText; maximumLineCount: 2; elide: Text.ElideRight; wrapMode: Text.WordWrap; color: Theme.color.textPrimary; font.pointSize: Type.titleSmall; font.weight: Type.bold }
                AlbumArtistLine { width: parent.width; document: dialog.info; onFollowed: dialog.close() }
            }
            Row {
                id: headerActions; anchors.right: parent.right; spacing: Metrics.gap8
                IconButton {
                    glyph: Icons.copy; tip: I18n.tr("Copy", I18n.rev)
                    onClicked: copyMenu.open()
                    Menu {
                        id: copyMenu; objectName: "albumCopyMenu"; y: parent.height + Metrics.gap4
                        entries: dialog.info.copy_actions || []
                        onChosen: index => clipboard.write(index === 0 ? dialog.info.copy_basic || "" : dialog.activeTab === 0 ? dialog.info.copy_credits || "" : (dialog.info.copy_header || "") + "\n\n" + review.getText(0, review.length))
                    }
                }
                IconButton { id: dismiss; objectName: "closeAlbumInformation"; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
            }
        }
        Row {
            width: parent.width; height: parent.height - title.height - Metrics.gap16
            spacing: Metrics.gap24
            ScrollView {
                id: metadata
                width: Math.min(Metrics.heroArt, parent.width / 3); height: parent.height // derived: reserve two thirds for the reading pane
                clip: true; contentWidth: availableWidth
                ScrollBar.vertical: ViewportScrollBar {}
                Column {
                    width: metadata.availableWidth; spacing: Metrics.gap12
                    ArtTile { width: parent.width; height: width; source: dialog.art }
                    Text { width: parent.width; visible: !!dialog.info.label; text: I18n.tr("Label", I18n.rev); color: Theme.color.textTertiary; font.pointSize: Type.meta }
                    TextEdit {
                        objectName: "albumLabel"; width: parent.width; visible: !!dialog.info.label; text: dialog.info.label || ""; readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap
                        color: labelHover.hovered && /^(artist|label):[^:]+/.test(dialog.info.label_ref || "") ? Theme.color.accentHover : Theme.color.textPrimary; selectionColor: Theme.color.accent; font.pointSize: Type.body
                        HoverHandler { id: labelHover; cursorShape: /^(artist|label):[^:]+/.test(dialog.info.label_ref || "") ? Qt.PointingHandCursor : Qt.IBeamCursor }
                        TapHandler { enabled: /^(artist|label):[^:]+/.test(dialog.info.label_ref || ""); onTapped: dialog.follow(dialog.info.label_ref) }
                    }
                    Repeater {
                        model: [dialog.info.released || dialog.info.release_date || "", dialog.info.genre || "", dialog.summary, dialog.info.quality_label || "", dialog.info.copyright || "", dialog.info.upc ? "UPC: " + dialog.info.upc : ""]
                        TextEdit { required property string modelData; width: parent.width; visible: modelData !== ""; text: modelData; readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap; color: Theme.color.textSecondary; selectionColor: Theme.color.accent; font.pointSize: Type.body }
                    }
                    Flow {
                        width: parent.width; spacing: Metrics.gap8
                        Repeater {
                            model: dialog.info.external_links || []
                            IconButton {
                                required property var modelData
                                objectName: "albumExternalLink"
                                tip: modelData.name
                                SourceIcon {
                                    anchors.centerIn: parent; width: Metrics.iconMd; height: width
                                    monochrome: modelData.name === "Discogs"
                                    sourceInfo: ({icon: "qrc:/qt/qml/app/chilpa/kit/qml/Theme/../../assets/icons/brands/" + ({MusicBrainz:"musicbrainz",Discogs:"discogs","Last.fm":"lastfm"})[modelData.name] + ".svg"})
                                }
                                onClicked: {
                                    externalMenu.url = modelData.url
                                    const at = mapToItem(externalMenu.parent, 0, height + Metrics.gap4)
                                    externalMenu.x = at.x; externalMenu.y = at.y; externalMenu.open()
                                }
                            }
                        }
                    }
                        Repeater {
                            model: (dialog.info.booklets || []).filter(b => /^https?:\/\//.test(b.url || ""))
                        PillButton { required property var modelData; text: modelData.name || I18n.tr("Booklet", I18n.rev); onClicked: Qt.openUrlExternally(modelData.url) }
                    }
                }
            }
            Column {
                width: parent.width - metadata.width - parent.spacing; height: parent.height
                spacing: Metrics.gap12
                SegmentedControl {
                    id: tabs; objectName: "albumInformationTabs"
                    options: [{text:I18n.tr("Credits", I18n.rev)}, {text:I18n.tr("Review", I18n.rev)}]
                    current: dialog.activeTab
                    onActivated: index => dialog.activeTab = index
                }
                ScrollView {
                    id: reading; objectName: "albumInformationScroll"
                    width: parent.width; height: parent.height - tabs.height - parent.spacing
                    contentWidth: availableWidth; clip: true
                    ScrollBar.vertical: ViewportScrollBar {}
                    Column {
                        width: reading.availableWidth; spacing: Metrics.gap12
                        Loader {
                            objectName: "albumCreditsLoader"
                            width: parent.width; active: dialog.activeTab === 0; visible: active
                            sourceComponent: Column {
                                width: parent.width; spacing: Metrics.gap8
                                Repeater {
                                    model: dialog.info.track_credits || []
                                    AlbumCreditRow { required property var modelData; width: parent.width; credit: modelData; onFollow: artist => { dialog.close(); Navigation.goKey(artist.target, artist.key) } }
                                }
                                Text { width: parent.width; visible: !(dialog.info.track_credits || []).length; text: I18n.tr("No credits available", I18n.rev); wrapMode: Text.WordWrap; color: Theme.color.textSecondary; font.pointSize: Type.body }
                            }
                        }
                        TextEdit {
                            id: review; objectName: "albumReview"; visible: dialog.activeTab === 1; width: parent.width
                            text: TextLinks.render(dialog.info.description || I18n.tr("No review available", I18n.rev), "", Theme.color.textPrimary, Theme.color.textPrimary)
                            textFormat: TextEdit.AutoText; readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap
                            color: Theme.color.textPrimary; selectionColor: Theme.color.accent; font.pointSize: Type.body
                            onLinkActivated: link => { if (/^https?:\/\//.test(link)) Qt.openUrlExternally(link) }
                        }
                    }
                }
            }
        }
    }
}
