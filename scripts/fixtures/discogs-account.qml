import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.about
import app.chilpa.metadata
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

import "qrc:/qt/qml/app/chilpa/metadata/qml/Components" as AccountParts

import "qrc:/qt/qml/app/chilpa/qml" as Shell

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
    Component { id: mainTemplate; Shell.Main {} }
    Component { id: quotaTemplate; AccountParts.QuotaDialog {} }
    function check(condition, message) { if (!condition) throw new Error(message) }
    property var actions
    function row(key) {
        const seen = []
        function visit(item) {
            if (!item || seen.includes(item)) return null
            seen.push(item)
            if (item.key === key && item.control !== undefined && item.visible) return item
            for (const child of item.children || []) { const found = visit(child); if (found) return found }
            return null
        }
        return visit(mainWindow?.contentItem)
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
            mainWindow = mainTemplate.createObject(null)
            check(!!mainWindow,"real Main did not construct")
            mainWindow.width = 1100; mainWindow.height = 850
            Qt.createQmlObject('import QtQuick; Rectangle { anchors.fill: parent; z: -100 }',mainWindow.contentItem).color = Qt.binding(() => mainWindow.color)
            focusAccount();phase = 1;return
        }
        check(!Player.playing,"account settings started playback")
        if (phase === 1) {
            if (DiscogsAccount.busy || !row("integrations.discogs.account") || !row("integrations.discogs.token")) return
            actions = row("integrations.discogs.account").page
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
            quota = quotaTemplate.createObject(mainWindow.contentItem)
            check(!!quota,"quota dialog did not construct")
            quota.open();cycleAt = Date.now();phase = 4;return
        }
        if (phase === 4) {
            if (Date.now() - cycleAt < 1000) return
            check(quota.opened,"quota dialog did not open")
            capture("SMOKE_QUOTA_CAPTURE",5);return
        }
        if (phase === 5) {
            quota.close()
            actions.set("integrations.offline","true")
            check(row("integrations.discogs.account")?.control === "fixed","offline kept browser sign-in active")
            check(row("integrations.discogs.token")?.control === "fixed","offline kept token verification active")
            check(!row("integrations.discogs.code"),"offline revealed an absent browser code")
            actions.set("integrations.offline","false");actions.set("integrations.proxy.enabled","false")
            check(row("integrations.discogs.account")?.control === "none","proxy opt-out kept browser setup active")
            check(row("integrations.discogs.token")?.control === "secret","proxy opt-out disabled direct personal tokens")
            check(!row("integrations.discogs.code"),"proxy opt-out revealed the browser code")
            actions.set("integrations.proxy.enabled","true")
            actions = null;Navigation.go("home",0);cycles++;cycleAt = Date.now();phase = 6;return
        }
        if (phase === 6) {
            check(DiscogsAccount.revision === revision && !DiscogsAccount.busy,"idle account state changed")
            check(!MetadataEditor.discogsQuota,"preview created a quota episode")
            if (Date.now() - began >= duration) {
                console.log("SMOKE Discogs real Main completed cycles=" + cycles)
                stopped = true;Qt.exit(0);return
            }
            if (Date.now() - cycleAt >= 110000) {focusAccount();cycleAt = Date.now();phase = 7}
        }
        if (phase === 7 && Date.now() - cycleAt >= 1500) {
            if (!row("integrations.discogs.account")) return
            check(row("integrations.discogs.account")?.control === "button","account action did not reopen")
            actions = null;Navigation.go("home",0);cycles++;cycleAt = Date.now();phase = 6
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
