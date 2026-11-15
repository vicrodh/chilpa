import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "../../qml/Screens"

TestCase {
    id:test
    name:"FolderMetadata"; when:windowShown; visible:true; width:1200; height:900
    Component { id:maker; FolderMetadataScreen { width:1200; height:900 } }
    function init() { failOnWarning(/ReferenceError|TypeError|Binding loop|invalid context/) }
    function test_folder_paths_open_without_playing_or_writing() {
        const paths=["/music/Avantasia","/music/Iron Maiden"]
        const page=createTemporaryObject(maker,this,{routeKey:JSON.stringify(paths)})
        const model=findChild(page,"folderMetadataModel")
        compare(model.requests[0],{action:"open",value:paths})
        compare(model.saves,0)
        verify(!findChild(page,"batchSave").enabled)
    }
    function test_genres_default_to_add_and_artist_selection_is_atomic() {
        const page=createTemporaryObject(maker,this,{routeKey:'["/music"]'})
        const model=findChild(page,"folderMetadataModel")
        compare(findChild(page,"batchOperation").value,"append_unique")
        findChild(page,"batchArtists").text="Iron Maiden\nJudas Priest"
        waitForRendering(page)
        mouseClick(findChild(page,"batchChooseArtists"))
        tryVerify(()=>model.requests.some(row=>row.action==="select"))
        const selected=model.requests.find(row=>row.action==="select")
        compare(selected.value,{mode:"artists",artists:["Iron Maiden","Judas Priest"],selected:true,replace:true})
        findChild(page,"batchValue").text="N.W.O.B.H.M"
        waitForRendering(page)
        mouseClick(findChild(page,"batchPropose"))
        tryVerify(()=>model.requests.some(row=>row.action==="patch"))
        const patch=model.requests.find(row=>row.action==="patch")
        compare(patch.value,{field:"genres",operation:"append_unique",value:["N.W.O.B.H.M"]})
        compare(model.saves,0,"preview does not write files")
    }
    function test_hidden_and_settled_views_release_polling_and_large_lists_are_virtual() {
        const page=createTemporaryObject(maker,this,{routeKey:'["/music"]'})
        const model=findChild(page,"folderMetadataModel")
        const data=JSON.parse(model.document)
        data.counts.files=20000;data.busy=true
        model.publish(data)
        tryVerify(()=>Pulse.slowHeld>0)
        page.visible=false
        tryCompare(Pulse,"slowHeld",0)
        const calls=model.polls
        wait(200);compare(model.polls,calls)
        page.visible=true;data.busy=false;model.publish(data)
        tryCompare(Pulse,"slowHeld",0)
        const list=findChild(page,"batchPreview")
        tryCompare(list,"count",20000)
        verify(list.contentItem.children.length<100)
        list.contentY=10000
        tryVerify(()=>model.requests.some(row=>row.action==="seek" && row.first>0))
        verify(list.contentItem.children.length<100)
    }
    function test_filtering_files_also_scopes_selection_and_field_proposals() {
        const page=createTemporaryObject(maker,this,{routeKey:'["/music"]'})
        const model=findChild(page,"folderMetadataModel")
        page.browse("items",{key:"1:g-a",title:"Album A"})
        page.select({mode:"all",selected:false})
        page.patch({field:"genres",operation:"append_unique",value:["Symphonic Metal"]})
        const selected=model.requests.find(row=>row.action === "select")
        const patch=model.requests.find(row=>row.action === "patch")
        compare(selected.value.groupKey,"1:g-a")
        compare(patch.value.groupKey,"1:g-a")
        compare(model.saves,0)
    }
    function test_a_busy_or_failed_open_can_be_requested_again() {
        const page=createTemporaryObject(maker,this,{routeKey:'["/music"]'})
        const model=findChild(page,"folderMetadataModel")
        model.publish({state:"unavailable",error:"Wait for the previous selection",counts:{files:0,selected:0},beets:{ready:true}})
        const button=findChild(page,"batchReopen")
        verify(button.visible && button.enabled)
        button.clicked()
        compare(model.requests.filter(row=>row.action === "open").length,2)
        compare(model.saves,0)
    }
    function test_disc_plans_name_groups_and_positions_without_writing() {
        const page=createTemporaryObject(maker,this,{routeKey:'["/music"]'})
        const model=findChild(page,"folderMetadataModel")
        page.groupChosen({key:"1:g-a",title:"Album",artist:"Artist",discs:[{number:1}]})
        page.groupChosen({key:"1:g-b",title:"Album bonus",artist:"Artist",discs:[{number:1}]})
        page.browse("groups")
        const grouping=findChild(page,"batchGrouping")
        tryCompare(grouping,"discs",[{groupKey:"1:g-a",sourceDisc:1,number:1,title:"",album:"Album",trackNumberMode:"keep"},
            {groupKey:"1:g-b",sourceDisc:1,number:2,title:"",album:"Album bonus",trackNumberMode:"keep"}])
        grouping.propose()
        const plan=model.requests.find(row=>row.action === "groupPlan")
        compare(plan.value.groups,["1:g-a","1:g-b"])
        compare(plan.value.discs[1].number,2)
        compare(model.saves,0)
    }
    function test_candidate_fields_need_a_new_preview_after_selection_changes() {
        const page=createTemporaryObject(maker,this,{routeKey:'["/music"]'})
        const model=findChild(page,"folderMetadataModel")
        page.chooseCandidate({key:"1:lookup:0",groupKey:"1:g-a"},[])
        compare(model.requests.find(row=>row.action === "candidate").value.fields,[])
        const data=JSON.parse(model.document)
        data.dataRevision=9
        data.plan={id:"1:p1",baseDataRevision:9,kind:"candidate",canApply:true,genresOperation:"append_unique",fieldChoices:[{key:"genres",field:"genres",label:"Genres",selected:true}]}
        model.publish(data)
        const proposal=findChild(page,"batchProposal")
        tryVerify(()=>proposal.fieldsMatch)
        verify(findChild(page,"batchApplyPlan").enabled)
        proposal.choices=[{key:"genres",field:"genres",label:"Genres",selected:false}]
        verify(!proposal.fieldsMatch)
        verify(!findChild(page,"batchApplyPlan").enabled)
        proposal.choices=[{key:"genres",field:"genres",label:"Genres",selected:true}]
        data.dataRevision=10;model.publish(data)
        tryVerify(()=>!findChild(page,"batchApplyPlan").enabled)
        compare(model.saves,0)
    }
    function test_a_join_preserves_distinct_existing_disc_numbers_by_default() {
        const page=createTemporaryObject(maker,this,{routeKey:'["/music"]'})
        page.groupChosen({key:"1:g-b",title:"Album CD2",artist:"Artist",discs:[{number:2}]})
        page.groupChosen({key:"1:g-a",title:"Album CD1",artist:"Artist",discs:[{number:1}]})
        const grouping=findChild(page,"batchGrouping")
        tryVerify(()=>grouping.discs.length === 2)
        compare(grouping.discs.map(disc=>disc.number),[2,1])
    }
    function test_a_disc_plan_does_not_claim_that_it_has_no_metadata_changes() {
        const page=createTemporaryObject(maker,this,{routeKey:'["/music"]'})
        const model=findChild(page,"folderMetadataModel")
        const data=JSON.parse(model.document)
        data.dataRevision=9
        data.plan={id:"1:p1",baseDataRevision:9,kind:"grouping",canApply:true,title:"Joined record",counts:{files:4,changed:4,previewRows:4},resultGroups:[{title:"Joined record",artist:"Artist",files:4}]}
        model.publish(data)
        const proposal=findChild(page,"batchProposal")
        tryVerify(()=>proposal.visible)
        verify(!findChild(page,"batchCandidateChanges").visible)
        verify(findChild(page,"batchApplyPlan").enabled)
        compare(model.saves,0)
    }
    function test_global_progress_keeps_the_full_operation_when_the_preview_is_filtered() {
        const page=createTemporaryObject(maker,this,{routeKey:'["/music"]'})
        const model=findChild(page,"folderMetadataModel")
        model.progress=JSON.stringify({session:1,operationId:3,revision:5,phase:"saving",completed:7,total:20,totalKnown:true,saved:6,failed:1})
        const bar=findChild(page,"batchProgressBar")
        tryCompare(bar,"value",0.35)
        model.count=1;model.groupKey="one-album"
        compare(bar.value,0.35,"the visible preview must not replace the operation total")
        compare(findChild(page,"batchProgressCounts").text,"7 of 20 files processed")
        compare(model.saves,0)
    }
    function test_cancelled_progress_keeps_partial_results_and_finalizing_does_not_claim_completion() {
        const page=createTemporaryObject(maker,this,{routeKey:'["/music"]'})
        const model=findChild(page,"folderMetadataModel")
        model.progress=JSON.stringify({session:1,operationId:3,revision:5,phase:"finalizing",completed:4,total:4,totalKnown:true,saved:3,failed:1})
        tryCompare(findChild(page,"batchProgressTitle"),"text","Updating the library…")
        compare(findChild(page,"batchProgressBar").value,0)
        model.progress=JSON.stringify({session:1,operationId:4,revision:6,phase:"cancelled",completed:2,total:4,totalKnown:true,saved:1,failed:1,cancelled:true})
        tryCompare(findChild(page,"batchProgressBar"),"value",0.5)
        compare(findChild(page,"batchProgressTitle").text,"Operation cancelled")
        compare(model.saves,0)
    }
}
