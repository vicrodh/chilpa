import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"

Column {
    id: details
    property double cardId: 0
    property var trackModel
    property bool selecting: false
    signal artwork()
    signal selectToggle()
    signal context(real left, real top)
    property var record
    readonly property var info: cardId < 0 ? JSON.parse(Sources.card(cardId, Library.remoteRev) || "{}") : ({})
    readonly property bool canFavorite: /^(album|playlist|artist):/.test(info.ref || "") && info.editable !== true
    signal information()
    readonly property real heroHeight: heading.height
    visible: cardId < 0 && !!info.plugin
    spacing: Metrics.gap8
    function open(reference) { heading.follow(reference) }
    onInfoChanged: if (playlistEditor.visible) playlistEditor.info = info
    SourcePlaylistEditor { id: playlistEditor }
    SourcePageHeader { id: heading; record: details.record; selecting: details.selecting; onArtwork: details.artwork(); onSelectToggle: details.selectToggle(); onContext: (left, top) => details.context(left, top); width: parent.width; currentPage: true; externalInformation: true; onInformation: details.information(); contrasted: SettingsModel.albumHeader !== "off" && !!details.record && String(details.record.art) !== ""; cardId: details.cardId; playbackModel: details.trackModel || ({count:0,summary:""}); info: Object.assign({}, details.info, {title: details.record ? details.record.title : details.info.title, art: details.record ? details.record.art : details.info.art}) }
    Flow {
        width: parent.width
        spacing: Metrics.gap8
        PillButton { visible: !!details.info.label_ref; text: details.info.label || I18n.tr("Label", I18n.rev); onClicked: details.open(details.info.label_ref) }
        PillButton { visible: details.info.editable === true; text: I18n.tr("Edit playlist", I18n.rev); onClicked: playlistEditor.begin(details.info, "update") }
        PillButton { visible: details.info.copyable === true; text: I18n.tr("Copy playlist", I18n.rev); onClicked: playlistEditor.begin(details.info, "copy") }
    }
}
