import QtQuick
import app.chilpa.controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"

// One pending choice reuses the shared confirmation; its body exists only while open.
Item {
    id: host
    property bool active: true
    property var controller: AudioOutput
    property var document: ({})
    property var pending: ({})
    signal requested()
    function presentPending() {
        if (!active || question.visible) return
        document=pending
        if (document.token) { requested();question.open() }
    }
    function refresh() {
        if (!active) return
        pending=JSON.parse(controller.channelOffer() || "{}")
        if (question.visible) {
            if (pending.token === document.token) { document=pending;return }
            question.answered=true;question.close();return
        }
        presentPending()
    }
    onActiveChanged: if(active) refresh();else if(question.opened) { question.answered=true;question.close() }
    Component.onCompleted: refresh()
    Connections { target:host.active ? host.controller : null; function onChannelOfferChanged() { host.refresh() } }
    Connections { target:question;function onClosed() { Qt.callLater(host.presentPending) } }
    Connections { target:I18n;function onRevChanged() { host.refresh() } }
    ConfirmDialog {
        id: question
        objectName: "channelConversionOffer"
        title: I18n.tr("Play in stereo?",I18n.rev)
        message: host.document.message || ""
        confirmText: I18n.tr("Play in stereo",I18n.rev)
        cancelText: I18n.tr("Not now",I18n.rev)
        option: I18n.tr("Remember for this output",I18n.rev)
        targetValid:!!host.document.token && host.document.token === host.pending.token
        checkTarget:()=>host.controller.channelCurrent(host.document.token || "")
        onConfirmed: host.controller.channelAnswer(host.document.token || "",true,optionChecked)
        onDismissed: host.controller.channelAnswer(host.document.token || "",false,false)
    }
}
