import QtQuick
import QtQml.Models
import app.chilpa
import app.chilpa.about
import app.chilpa.metadata

Item {
    id: smoke
    property var mainWindow
    property var quota
    property int phase: 0
    property bool stopped: false
    property int duration: SMOKE_DURATION
    property double began: Date.now()
    property double cycleAt: 0
    property int cycles: 0
    property int revision: 0
    SettingsPage { id: settings; section: "integrations" }
    Instantiator {
        id: rows
        model: settings
        delegate: QtObject {
            required property string key
            required property string control
            required property string value
        }
    }
    function check(condition, message) { if (!condition) throw new Error(message) }
    function row(key) {
        for (let i = 0; i < rows.count; ++i) if (rows.objectAt(i).key === key) return rows.objectAt(i)
        return null
    }
    function focusAccount() {
        Visit.keep("settings.focus", {section:"integrations",key:"integrations.discogs.account"})
        Navigation.openSection("integrations")
    }
    function capture(path, next) {
        mainWindow.contentItem.grabToImage(result => {
            check(result.saveToFile(path),"native capture could not be saved")
            phase = next
        })
        phase = -1
    }
    function advance() {
        if (phase === 0) {
            Onboarding.command("close")
            const component = Qt.createComponent("qrc:/qt/qml/app/chilpa/qml/Main.qml")
            check(component.status === Component.Ready,component.errorString())
            mainWindow = component.createObject(null)
            check(!!mainWindow,"real Main did not construct")
            mainWindow.width = 1100; mainWindow.height = 850
            Qt.createQmlObject('import QtQuick; Rectangle { anchors.fill: parent; z: -100 }',mainWindow.contentItem).color = Qt.binding(() => mainWindow.color)
            focusAccount();phase = 1;return
        }
        check(!Player.playing,"account settings started playback")
        if (phase === 1) {
            DiscogsAccount.refresh()
            if (DiscogsAccount.busy) return
            settings.refresh()
            check(row("integrations.discogs.account")?.control === "button","account action is absent")
            check(row("integrations.discogs.token")?.control === "secret","personal token input is absent")
            check(!row("integrations.discogs.code"),"browser code is visible without authorization")
            revision = DiscogsAccount.revision
            cycleAt = Date.now();phase = 2;return
        }
        if (phase === 2) {
            if (Date.now() - cycleAt < 1500) return
            capture("SMOKE_CAPTURE",3);return
        }
        if (phase === 3) {
            const component = Qt.createComponent("qrc:/qt/qml/app/chilpa/metadata/qml/Components/QuotaDialog.qml")
            check(component.status === Component.Ready,component.errorString())
            quota = component.createObject(mainWindow.contentItem)
            check(!!quota,"quota dialog did not construct")
            quota.open();cycleAt = Date.now();phase = 4;return
        }
        if (phase === 4) {
            if (Date.now() - cycleAt < 1000) return
            check(quota.opened,"quota dialog did not open")
            capture("SMOKE_QUOTA_CAPTURE",5);return
        }
        if (phase === 5) {
            quota.close();quota.destroy();quota = null
            Navigation.open("home",0);cycles++;cycleAt = Date.now();phase = 6;return
        }
        if (phase === 6) {
            DiscogsAccount.refresh()
            check(DiscogsAccount.revision === revision && !DiscogsAccount.busy,"idle account state changed")
            check(!MetadataEditor.discogsQuota,"preview created a quota episode")
            if (Date.now() - began >= duration) {
                console.log("SMOKE Discogs real Main completed cycles=" + cycles)
                stopped = true;Qt.exit(0);return
            }
            if (Date.now() - cycleAt >= 110000) {focusAccount();cycleAt = Date.now();phase = 7}
        }
        if (phase === 7 && Date.now() - cycleAt >= 1500) {
            settings.refresh()
            check(row("integrations.discogs.account")?.control === "button","account action did not reopen")
            Navigation.open("home",0);cycles++;cycleAt = Date.now();phase = 6
        }
    }
    Timer {
        interval: 250;repeat: true;running: !smoke.stopped
        onTriggered: {
            try {smoke.advance()}
            catch (error) {smoke.stopped = true;console.error("SMOKE FAIL Discogs phase=" + smoke.phase + " " + error.message);Qt.exit(1)}
        }
    }
    Timer {
        interval: smoke.duration + 60000;running: !smoke.stopped
        onTriggered: {console.error("SMOKE FAIL Discogs timeout phase=" + smoke.phase);Qt.exit(1)}
    }
}
