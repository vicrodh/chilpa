import QtQuick
import app.chilpa.controls
import "../Components"

// The window's one place where a track a list's menu chose shows what is
// known of it. The menu only asks: nothing plays and the queue stays as it
// was. The modal exists only while it is open.
Item {
    id: host
    property bool listenForRequests: true
    property string path: ""
    property string title: ""
    property bool shown: false
    property bool ownsSubject: false
    function releaseSubject() {
        if (!ownsSubject) return
        ownsSubject=false
        NowAbout.pinSubject("")
    }
    onShownChanged: if (!shown) releaseSubject()
    Component.onDestruction: releaseSubject()

    Connections {
        target: NowAbout
        enabled: host.listenForRequests
        function onTrackInfoRequested(path, title) {
            NowAbout.pinSubject(path)
            host.ownsSubject=true
            host.path = path
            host.title = title
            host.shown = true
        }
    }
    Loader {
        objectName: "trackInfoRequestHost"
        active: host.shown && host.path !== ""
        sourceComponent: TrackInfoModal {
            subject: host.path
            subjectTitle: host.title
            Component.onCompleted: open()
            onClosed: host.shown = false
        }
    }
}
