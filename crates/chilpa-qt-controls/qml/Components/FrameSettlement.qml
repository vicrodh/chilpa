import QtQuick
import app.chilpa.kit

// I settle the matching publication after its frame and route preparation.
Item {
    id: settlement
    property var navigation: null
    property var router: null
    property double readyRevision: -1

    function settle() {
        if (navigation?.pending && readyRevision === navigation.pendingRevision
                && router?.phase !== "acknowledging")
            Qt.callLater(apply, readyRevision)
    }
    function apply(revision) {
        if (navigation?.pending && readyRevision === revision && navigation.pendingRevision === revision
                && router?.phase !== "acknowledging") navigation.settle(revision)
    }
    FrameFence {
        id: frame
        onPresented: token => {
            if (token !== settlement.navigation?.pendingRevision) return
            settlement.readyRevision = token
            settlement.settle()
        }
    }
    Connections {
        target: settlement.navigation
        function onPendingRevisionChanged() {
            settlement.readyRevision = -1
            frame.arm(settlement.navigation.pendingRevision)
        }
    }
    Connections {
        target: settlement.router
        function onPreparingRoute() { settlement.settle() }
    }
    Component.onCompleted: if (navigation?.pending) frame.arm(navigation.pendingRevision)
}
