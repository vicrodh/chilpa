import QtQuick
import QtTest
import app.chilpa
import app.chilpa.controls
import "../../../chilpa-qt-kit/qml/Theme"
import "../../qml/Components"

TestCase {
    name: "QueueAbout"
    when: windowShown
    visible: true
    width: 360
    height: 600

    QueueAbout { id: panel; width: 340; height: 580 }

    function init() {
        panel.visible = true
        panel.retryRevision = 0
        panel.part = "track"
        Player.path = "/test.wav"
        NowAbout.retried = ({})
        NowAbout.allowRetry = true
        NowAbout.allowRefetch = true
        NowAbout.refetchedPath = ""
        NowAbout.trackAnswer = ({ loading: false, facts: [], credits: [], description: "" })
        NowAbout.albumAnswer = ({ loading: false, facts: [], credits: [], description: "" })
    }

    function test_hidden_information_does_not_request_new_tracks() {
        panel.visible = false
        const trackCalls = NowAbout.calls.track
        const albumCalls = NowAbout.calls.album
        Player.path = "another-track"
        Library.remoteRev++
        wait(30)
        compare(NowAbout.calls.track, trackCalls)
        compare(NowAbout.calls.album, albumCalls)
    }

    function test_a_failed_request_does_not_claim_that_nothing_is_known() {
        NowAbout.trackAnswer = ({ loading: false, failed: true, facts: [], credits: [] })
        tryCompare(findChild(panel, "queueAboutFailure"), "visible", true)
        compare(findChild(panel, "queueAboutNothing").visible, false)
        NowAbout.trackAnswer = ({ loading: false, facts: [], credits: [] })
        tryCompare(findChild(panel, "queueAboutNothing"), "visible", true)
    }

    function test_retry_targets_the_visible_failed_track_or_album_and_finishes_in_loading() {
        for (const part of ["track", "album"]) {
            panel.part = part
            if (part === "track") NowAbout.trackAnswer = ({loading: false, failed: true})
            else NowAbout.albumAnswer = ({loading: false, failed: true})
            const retry = findChild(panel, "queueAboutRetry")
            tryCompare(retry, "visible", true)
            waitForPolish(panel)
            waitForRendering(retry)
            mouseClick(retry, retry.width / 2, retry.height / 2)
            compare(NowAbout.retried.part, part)
            compare(NowAbout.retried.path, "/test.wav")
            tryCompare(retry, "visible", false)
            compare(panel.found.loading, true)
        }
        compare(panel.retryRevision, 2)
    }

    function test_a_refused_retry_does_not_replace_the_error_and_empty_answers_offer_no_retry() {
        NowAbout.trackAnswer = ({loading: false, failed: true})
        NowAbout.allowRetry = false
        const retry = findChild(panel, "queueAboutRetry")
        tryCompare(retry, "visible", true)
        waitForPolish(panel)
        waitForRendering(retry)
        mouseClick(retry, retry.width / 2, retry.height / 2)
        compare(panel.retryRevision, 0)
        verify(panel.found.failed)
        NowAbout.trackAnswer = ({loading: false, facts: [], credits: []})
        tryCompare(retry, "visible", false)
    }

    function test_the_browser_action_is_a_lucide_external_link_icon() {
        NowAbout.trackAnswer = ({loading: false, origin: "MusicBrainz", link: "https://musicbrainz.org", facts: [], credits: []})
        const browser = findChild(panel, "queueAboutLink")
        tryCompare(browser, "visible", true)
        compare(browser.glyph, Lucide.externalLink)
        compare(browser.iconFamily, Lucide.font)
        compare(browser.width, Metrics.button28)
    }

    function test_recording_and_album_credits_are_separate_and_existing_facts_survive_a_network_failure() {
        NowAbout.trackAnswer = ({ loading: false, failed: true,
            facts: [{label: "Composer", value: "Tagged composer"}],
            credits: [{name: "Bob Keane", roles: "Producer"}, {name: "Larry Levine", roles: "Recording engineer"}],
            origin: "MusicBrainz" })
        tryCompare(findChild(panel, "queueAboutCredits"), "count", 2)
        compare(findChild(panel, "queueAboutFacts").count, 1)
        compare(findChild(panel, "queueAboutNothing").visible, false)
        NowAbout.albumAnswer = ({ loading: false, facts: [], credits: [{name: "Steve Hoffman", roles: "Compiler"}] })
        panel.part = "album"
        tryCompare(findChild(panel, "queueAboutCredits"), "count", 1)
        compare(panel.credits[0].name, "Steve Hoffman")
    }
    function test_refetch_is_explicit_album_only_and_preserves_visible_information() {
        NowAbout.albumAnswer = ({loading:false, can_refetch:true, facts:[{label:"Label", value:"Known label"}], credits:[{name:"Producer", roles:"Producer"}]})
        const refetch = findChild(panel, "queueAboutRefetch")
        compare(refetch.visible, false)
        panel.part = "album"
        tryCompare(refetch, "visible", true)
        compare(refetch.glyph, Lucide.rotateCcw)
        refetch.forceActiveFocus()
        keyClick(Qt.Key_Return)
        compare(NowAbout.refetchedPath, Player.path)
        compare(panel.retryRevision, 1)
        tryCompare(refetch, "enabled", false)
        compare(panel.facts[0].value, "Known label")
        compare(panel.credits.length, 1)
        NowAbout.albumAnswer = Object.assign({}, NowAbout.albumAnswer, {loading:false, failed:true})
        tryCompare(findChild(panel, "queueAboutFailure"), "visible", true)
        compare(panel.facts[0].value, "Known label")
        NowAbout.albumAnswer = Object.assign({}, NowAbout.albumAnswer, {can_refetch:false})
        tryCompare(refetch, "visible", false)
    }

    function test_refetch_rejected_for_a_changed_identity_does_not_announce_loading() {
        panel.part = "album"
        NowAbout.albumAnswer = ({loading:false, can_refetch:true, facts:[{label:"Label", value:"Known label"}]})
        NowAbout.allowRefetch = false
        findChild(panel, "queueAboutRefetch").clicked()
        compare(panel.retryRevision, 0)
        compare(panel.found.loading, false)
        compare(panel.facts[0].value, "Known label")
    }

}
