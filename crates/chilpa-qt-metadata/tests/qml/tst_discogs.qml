import QtQuick
import QtTest
import app.chilpa
import app.chilpa.metadata
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../../qml/Patterns"

TestCase {
    id: test
    name: "DiscogsAccountAndQuota"
    when: windowShown
    visible: true
    width: 900; height: 700
    QtObject { id: rows; property int reads: 0; function refresh() { reads++ } }
    QtObject { id: owner; property bool visible: true; property int visibility: Window.Windowed }
    DiscogsSettings { id: settings; rows: rows; ownerWindow: owner }
    DiscogsQuotaHost { id: host; current: true }
    function init() {
        settings.current = false; DiscogsAccount.busy = false; DiscogsAccount.reads = 0; rows.reads = 0
        MetadataEditor.discogsQuota = false; MetadataEditor.quotaTaken = 0; host.current = true
        Visit.keep("settings.focus", null)
        Navigation.section = ""
        owner.visible = true; owner.visibility = Window.Windowed
    }
    function cleanup() { findChild(host,"discogsQuotaDialog").close(); wait(0) }
    function showQuota() { MetadataEditor.discogsQuota = true; tryCompare(findChild(host,"discogsQuotaDialog"),"opened",true) }
    function test_quota_is_consumed_once_and_not_now_closes_without_navigation() {
        showQuota()
        compare(MetadataEditor.quotaTaken,1)
        findChild(host,"discogsNotNow").clicked()
        tryCompare(findChild(host,"discogsQuotaDialog"),"opened",false)
        compare(Navigation.section,"")
        host.present()
        compare(MetadataEditor.quotaTaken,1)
    }
    function test_a_hidden_editor_preserves_the_pending_notice_until_shown() {
        host.current = false
        MetadataEditor.discogsQuota = true
        compare(MetadataEditor.quotaTaken,0)
        host.current = true
        tryCompare(findChild(host,"discogsQuotaDialog"),"opened",true)
        compare(MetadataEditor.quotaTaken,1)
    }
    function test_token_and_oauth_buttons_reveal_their_own_settings_rows() {
        showQuota();findChild(host,"discogsUseToken").clicked()
        compare(Navigation.section,"integrations")
        compare(Visit.recall("settings.focus",null).key,"integrations.discogs.token")
        showQuota();findChild(host,"discogsSignIn").clicked()
        compare(Visit.recall("settings.focus",null).key,"integrations.discogs.account")
    }
    function test_account_refresh_is_revisioned_and_only_follows_pending_visible_settings() {
        DiscogsAccount.busy = true
        const timer = findChild(settings,"discogsSettingsRefresh")
        compare(timer.running,false)
        wait(300);compare(DiscogsAccount.reads,0)
        settings.current = true
        tryCompare(timer,"running",true)
        DiscogsAccount.revision++
        tryCompare(rows,"reads",1)
        owner.visibility = Window.Minimized
        tryCompare(timer,"running",false)
        const minimizedReads = DiscogsAccount.reads
        wait(600);compare(DiscogsAccount.reads,minimizedReads)
        owner.visibility = Window.Windowed;owner.visible = false
        tryCompare(timer,"running",false)
        settings.current = false
        tryCompare(timer,"running",false)
        const reads = DiscogsAccount.reads
        wait(600);compare(DiscogsAccount.reads,reads)
        DiscogsAccount.revision++
        compare(rows.reads,1)
    }
    function test_model_refresh_leaves_the_notifying_stack_and_drops_a_hidden_publication() {
        settings.current = true
        wait(0)
        DiscogsAccount.revision++
        compare(rows.reads,0)
        tryCompare(rows,"reads",1)
        DiscogsAccount.revision++
        settings.current = false
        wait(0)
        compare(rows.reads,1)
    }
}
