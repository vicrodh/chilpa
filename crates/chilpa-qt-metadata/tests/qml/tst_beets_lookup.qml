import QtQuick
import QtTest
import app.chilpa
import app.chilpa.metadata
import "../../qml/Patterns"
import "../../qml/Components"

TestCase {
    name:"BeetsLookup"
    when:windowShown
    visible:true
    width:640; height:1400
    Component { id:maker; MetadataLookup { width:400; doc:MetadataEditor.view } }
    Component { id:rowMaker; MetadataChangeRow { width:260 } }
    function document(engine) {
        return {state:"ready",dirty:false,tracks:[{id:1},{id:2}],lookup:{engine:engine || "native",provider:"musicbrainz",
            runtime:"/fixture/beets/python",searching:false,loading:false,beets_busy:false,matched:2,
            hits:[{title:"<b>Found album</b>",artist:"Artist",date:"2001",tracks:2,score:93}],
            release:{title:"Found album",date:"2001",discs:1,url:""},changes:[
                {key:"album:title",field:"title",label:"Album",current:"Curated album",proposed:"Found album",selected:true},
                {key:"album:genres",field:"genres",label:"Genres",current:"Symphonic Metal",proposed:"Metal",selected:false}
            ]}}
    }
    function init() {
        Navigation.went=[]
        MetadataEditor.engineCalls=[]; MetadataEditor.runtimeCalls=[]; MetadataEditor.changesChosen=[]
        MetadataEditor.loadedHits=[]; MetadataEditor.probes=0; MetadataEditor.lookupCancels=0
        MetadataEditor.applied=0; MetadataEditor.saved=0; MetadataEditor.choosingBeetsRuntime=false
        MetadataEditor.show(document("native"))
    }
    function test_runtime_configuration_is_in_settings_and_keeps_the_real_source() {
        MetadataEditor.show(document("beets"))
        const page=createTemporaryObject(maker,this)
        compare(findChild(page,"metadataSearchEngine"),null)
        compare(findChild(page,"metadataBeetsRuntime"),null)
        compare(findChild(page,"metadataBeetsCheck"),null)
        compare(MetadataEditor.view.lookup.provider,"musicbrainz")
        const settings=findChild(page,"metadataBeetsSettings")
        settings.forceActiveFocus(); keyClick(Qt.Key_Return)
        compare(Navigation.went,["settings","library"])
        compare(MetadataEditor.applied,0)
        compare(MetadataEditor.saved,0)
    }
    function test_album_editor_can_choose_engine_without_applying_or_saving_tags() {
        const page=createTemporaryObject(maker,this)
        const choice=findChild(page,"metadataLookupEngine")
        verify(choice)
        compare(choice.current,0)
        choice.forceActiveFocus(); keyClick(Qt.Key_Right)
        compare(MetadataEditor.engineCalls,["beets"])
        compare(choice.current,1)
        keyClick(Qt.Key_Left)
        compare(MetadataEditor.engineCalls,["beets","native"])
        compare(choice.current,0)
        MetadataEditor.updateLookup({searching:true})
        compare(choice.enabled,false)
        compare(MetadataEditor.applied,0)
        compare(MetadataEditor.saved,0)
    }
    function test_cancelling_a_lookup_keeps_configuration_and_form_values() {
        const doc=document("beets")
        doc.lookup.searching=true; doc.lookup.beets_busy=true
        MetadataEditor.show(doc)
        const page=createTemporaryObject(maker,this)
        const cancel=findChild(page,"metadataLookupCancel")
        verify(cancel.visible)
        cancel.forceActiveFocus(); keyClick(Qt.Key_Return)
        compare(MetadataEditor.lookupCancels,1)
        compare(MetadataEditor.runtimeCalls.length,0)
        compare(MetadataEditor.applied,0)
        compare(MetadataEditor.saved,0)
    }
    function test_genres_require_a_deliberate_selection_and_apply_only_changes_the_form() {
        MetadataEditor.show(document("beets"))
        const page=createTemporaryObject(maker,this)
        let title
        tryVerify(() => {title=findChild(page,"metadataChange:album:title"); return !!title})
        const genres=findChild(page,"metadataChange:album:genres")
        verify(genres); compare(genres.checked,false)
        title.forceActiveFocus(); keyClick(Qt.Key_Space)
        compare(findChild(page,"metadataApplySelected").enabled,false)
        genres.forceActiveFocus(); keyClick(Qt.Key_Space)
        compare(MetadataEditor.changesChosen[1],{key:"album:genres",selected:true})
        const apply=findChild(page,"metadataApplySelected")
        verify(apply.enabled)
        apply.forceActiveFocus(); keyClick(Qt.Key_Return)
        compare(MetadataEditor.applied,1)
        compare(MetadataEditor.saved,0)
    }
    function test_release_choices_expose_the_match_score_and_open_by_keyboard() {
        const doc=document("beets")
        doc.lookup.hits[0].label="A long release label ".repeat(10)
        doc.lookup.hits[0].catalog_number="A long catalog number ".repeat(10)
        MetadataEditor.show(doc)
        const page=createTemporaryObject(maker,this)
        page.width=260
        let hit
        tryVerify(() => {hit=findChild(page,"metadataReleaseHit0"); return !!hit})
        verify(hit.detail.includes("93"))
        const title=findChild(hit,"metadataReleaseHitTitle")
        compare(title.textFormat,Text.PlainText)
        const score=findChild(hit,"metadataReleaseHitScore")
        waitForRendering(hit)
        verify(score.visible)
        compare(score.text,"93%")
        verify(!score.truncated)
        verify(score.x + score.width <= hit.width)
        verify(title.mapToItem(hit,0,0).x + title.width <= score.x)
        const disc=document("beets")
        disc.lookup.hits[0].discs=[2]
        MetadataEditor.show(disc)
        tryCompare(score,"text","93% · Disc 2")
        hit.forceActiveFocus(); keyClick(Qt.Key_Return)
        compare(MetadataEditor.loadedHits,[0])
    }
    function test_long_track_context_and_values_fit_inside_a_narrow_change_row() {
        const row=createTemporaryObject(rowMaker,this,{change:{key:"track:1:title",label:"Title",title:"Long track title ".repeat(40),
            current:"Curated title ".repeat(40),proposed:"Proposed title ".repeat(40),selected:true}})
        const check=findChild(row,"metadataChange:track:1:title")
        waitForRendering(row)
        verify(check.width <= row.width)
        verify(check.height < row.height)
    }
}
