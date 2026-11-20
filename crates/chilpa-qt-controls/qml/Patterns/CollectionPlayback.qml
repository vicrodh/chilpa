import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components" as Shell
import "../Components"

Flow {
    id: actions
    objectName: "collectionActions"

    required property var model
    required property string label
    required property bool available
    required property bool filtered
    required property string scope
    required property real availableWidth
    property bool showRandomAlbum: false

    readonly property real unwrappedWidth: play.implicitWidth + shuffle.implicitWidth + queueButton.width + 2 * spacing // derived: two gaps for the base controls
        + (showRandomAlbum ? randomAlbum.width + spacing : 0) // derived: one gap per adjoining control
    width: Math.min(unwrappedWidth, availableWidth)
    spacing: Metrics.gap8
    enabled: available && !QueueActivity.busy

    onScopeChanged: if (chooser.visible && !chooser.working) chooser.close()
    onAvailableChanged: if (!available && chooser.visible && !chooser.working) chooser.close()
    onVisibleChanged: if (!visible && chooser.visible && !chooser.working) chooser.close()

    function request(action, shuffled, title) {
        if (!enabled) return
        if (filtered) {
            apply(action, shuffled, 0)
        } else {
            chooser.action = action
            chooser.shuffled = shuffled
            chooser.title = title
            chooser.open()
        }
    }

    function apply(action, shuffled, limit) {
        if (!enabled) return
        model.queueCollection(action, shuffled, limit, label)
        if (chooser.visible) {
            QueueActivity.refresh()
            chooser.working = QueueActivity.busy
            if (chooser.working) return
            chooser.close()
        }
        finish(action)
    }

    function finish(action) {
        if (action === "buffer:new") Navigation.go("listenlist", 0)
    }

    Connections {
        target: QueueActivity
        enabled: chooser.working
        function onBusyChanged() {
            if (QueueActivity.busy) return
            chooser.working = false
            chooser.close()
            actions.finish(chooser.action)
        }
    }

    PillButton {
        id: play
        objectName: "collectionPlay"
        kind: "primary"
        filled: true
        glyph: Icons.playerPlayFilled
        text: I18n.tr("Play", I18n.rev)
        onClicked: actions.request("play", false, text)
    }
    PillButton {
        id: shuffle
        objectName: "collectionShuffle"
        glyph: Icons.arrowsShuffle
        text: I18n.tr("Shuffle", I18n.rev)
        onClicked: actions.request("play", true, text)
    }
    IconButton {
        id: randomAlbum
        objectName: "collectionRandomAlbum"
        visible: actions.showRandomAlbum
        glyph: Lucide.dices
        iconFamily: Lucide.font
        tip: I18n.tr("Random album", I18n.rev)
        round: true
        size: Metrics.pillHeight
        onClicked: actions.apply("random-album", false, 0)
    }
    IconButton {
        id: queueButton
        objectName: "collectionQueue"
        tip: I18n.tr("Queue options", I18n.rev)
        glyph: Icons.playlistAdd
        round: true
        size: Metrics.pillHeight
        onClicked: menu.show(x, y + height + Metrics.gap4)
    }
    Shell.QueueMenu {
        id: menu
        objectName: "collectionQueueMenu"
        collectionAction: (action, title) => actions.request(action, false, title)
    }
    CollectionSizeDialog {
        id: chooser
        onChosen: (limit) => actions.apply(action, shuffled, limit)
    }
}
