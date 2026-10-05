import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"

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
        close()
        if (/^(album|playlist):/.test(reference)) Navigation.go("album", Sources.referenceCard(plugin, reference))
        else Navigation.goKey("source", plugin + " native:" + reference)
    }
    Column {
        anchors.fill: parent
        spacing: Metrics.gap16
        Item {
            width: parent.width; height: Math.max(title.height, dismiss.height)
            Column {
                id: title; width: parent.width - dismiss.width - Metrics.gap16; spacing: Metrics.gap4
                Text { width: parent.width; text: dialog.info.title || ""; textFormat: Text.PlainText; maximumLineCount: 2; elide: Text.ElideRight; wrapMode: Text.WordWrap; color: Theme.color.textPrimary; font.pointSize: Type.titleSmall; font.weight: Type.bold }
                Text { width: parent.width; text: dialog.info.artist || ""; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.body }
            }
            IconButton { id: dismiss; objectName: "closeAlbumInformation"; anchors.right: parent.right; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
        }
        Row {
            width: parent.width; height: parent.height - title.height - Metrics.gap16
            spacing: Metrics.gap24
            ScrollView {
                id: metadata
                width: Math.min(Metrics.heroArt, parent.width / 3); height: parent.height // derived: reserve two thirds for the reading pane
                clip: true; contentWidth: availableWidth
                Column {
                    width: metadata.availableWidth; spacing: Metrics.gap12
                    ArtTile { width: parent.width; height: width; source: dialog.art }
                    Text { width: parent.width; visible: !!dialog.info.label; text: I18n.tr("Label", I18n.rev); color: Theme.color.textTertiary; font.pointSize: Type.meta }
                    TextEdit {
                        width: parent.width; visible: !!dialog.info.label; text: dialog.info.label || ""; readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap
                        color: Theme.color.textPrimary; selectionColor: Theme.color.accent; font.pointSize: Type.body
                    }
                    PillButton { visible: !!dialog.info.label_ref; text: I18n.tr("Open label", I18n.rev); onClicked: dialog.follow(dialog.info.label_ref) }
                    Repeater {
                        model: [dialog.info.release_date || "", dialog.info.genre || "", dialog.summary, dialog.info.quality_label || "", dialog.info.copyright || "", dialog.info.upc ? "UPC: " + dialog.info.upc : ""]
                        TextEdit { required property string modelData; width: parent.width; visible: modelData !== ""; text: modelData; readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap; color: Theme.color.textSecondary; selectionColor: Theme.color.accent; font.pointSize: Type.body }
                    }
                    Repeater {
                        model: dialog.info.booklets || []
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
                    Column {
                        width: reading.availableWidth; spacing: Metrics.gap12
                        Flow {
                            visible: dialog.activeTab === 0; width: parent.width; spacing: Metrics.gap8
                            Repeater {
                                model: dialog.info.credits || []
                                PillButton { required property var modelData; text: modelData.name || ""; enabled: !!modelData.id; onClicked: dialog.follow("artist:" + modelData.id) }
                            }
                        }
                        Loader {
                            objectName: "albumCreditsLoader"
                            width: parent.width; active: dialog.activeTab === 0; visible: active
                            sourceComponent: Column {
                                width: parent.width; spacing: Metrics.gap8
                                Repeater {
                                    model: dialog.info.track_credits || []
                                    AlbumCreditRow { required property var modelData; width: parent.width; credit: modelData }
                                }
                                Text { width: parent.width; visible: !(dialog.info.track_credits || []).length; text: I18n.tr("No credits available", I18n.rev); wrapMode: Text.WordWrap; color: Theme.color.textSecondary; font.pointSize: Type.body }
                            }
                        }
                        TextEdit {
                            objectName: "albumReview"; visible: dialog.activeTab === 1; width: parent.width
                            text: dialog.info.description || I18n.tr("No review available", I18n.rev)
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
