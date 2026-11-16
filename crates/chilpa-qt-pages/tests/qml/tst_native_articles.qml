import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import app.chilpa.pages
import "../../qml/Screens"
import "../../qml/Components"

TestCase {
    id:test
    name:"NativeArticles"
    when:windowShown
    width:1000;height:700;visible:true
    Component { id:pageFactory;SourceArticleScreen { width:test.width;height:test.height;source:"fixture native:story:one" } }
    Component { id:projectionFactory;ArticleProjection {} }
    SignalSpy { id:reads;target:Sources;signalName:"pageRequested" }
    function doc(blocks,extra) {
        return JSON.stringify({layout:"article",title:"A plain <b>title</b>",article:Object.assign({id:"one",authors:["Author"],published_at:"2026-11-16T12:00:00Z",summary:"A plain <script>summary</script>",blocks:blocks || [{type:"paragraph",text:"Paragraph <a href='https://example.test'>text</a>"}]},extra || {})})
    }
    function init() {
        failOnWarning(/.*/)
        Sources.pages={"fixture native:story:one":doc()}
        reads.clear()
    }
    function descendants(item,predicate) {
        let result=predicate(item) ? [item] : []
        for(const child of item.children || [])result=result.concat(descendants(child,predicate))
        return result
    }
    function test_plain_text_and_unrelated_revisions_do_not_reload_the_document() {
        const page=createTemporaryObject(pageFactory,test)
        const model=findChild(page,"nativeArticleModel")
        tryCompare(page,"count",2)
        tryVerify(()=>descendants(page,node=>node.objectName==="articleBlockText").length>0)
        for(const text of descendants(page,node=>node.objectName==="articleBlockText"))compare(text.textFormat,Text.PlainText)
        const original=model.document
        compare(model.openCount,1);compare(reads.count,0)
        Library.remoteRev++;Sources.revision++;wait(30)
        compare(model.document,original);compare(model.openCount,1);compare(reads.count,0)
    }
    function test_retire_keeps_the_outgoing_projection_and_close_releases_it() {
        const page=createTemporaryObject(pageFactory,test)
        const model=findChild(page,"nativeArticleModel")
        tryCompare(page,"count",2)
        const original=model.document
        page.leaving=true
        compare(model.document,original);compare(model.active,false)
        const calls=model.pollCount
        Sources.revision++;wait(1100);compare(model.pollCount,calls)
        page.visible=false
        tryCompare(model,"document","{}");tryCompare(page,"count",0)
        compare(Object.keys(model.leases).length,0)
    }
    function test_images_only_hold_demand_while_their_block_is_in_the_viewport() {
        const blocks=[]
        for(let i=0;i<40;i++)blocks.push({type:"paragraph",text:"A paragraph with enough lines to scroll. ".repeat(12)})
        blocks.push({type:"image",url:"https://example.test/far.jpg",width:600,height:600})
        Sources.pages={"fixture native:story:one":doc(blocks,{image:"https://example.test/cover.jpg",image_width:600,image_height:600})}
        const page=createTemporaryObject(pageFactory,test)
        const model=findChild(page,"nativeArticleModel")
        tryVerify(()=>model.watched.includes("https://example.test/cover.jpg"))
        verify(!model.watched.includes("https://example.test/far.jpg"))
        page.positionViewAtEnd()
        tryVerify(()=>model.watched.includes("https://example.test/far.jpg"))
        tryVerify(()=>!Object.values(model.leases).includes("https://example.test/cover.jpg"))
        const image=findChild(page,"articleImage")
        verify(image);compare(image.cache,false);verify(image.sourceSize.width<=2048)
        page.visible=false
        tryCompare(model,"document","{}");compare(Object.keys(model.leases).length,0)
    }
    function test_header_starts_at_the_top_and_releases_its_cover_when_scrolled_out() {
        const blocks=[]
        for(let i=0;i<20;i++)blocks.push({type:"paragraph",text:"A longer article paragraph. ".repeat(20)})
        Sources.pages={"fixture native:story:one":doc(blocks,{image:"https://example.test/cover.jpg",image_width:1200,image_height:700})}
        const page=createTemporaryObject(pageFactory,test)
        const model=findChild(page,"nativeArticleModel")
        tryCompare(page,"count",21)
        tryVerify(()=>Math.abs(page.contentY-(page.headerItem.y-page.topMargin))<1)
        tryVerify(()=>Object.values(model.leases).includes("https://example.test/cover.jpg"))
        page.contentY=0
        tryVerify(()=>!Object.values(model.leases).includes("https://example.test/cover.jpg"))
    }
    function test_a_short_article_also_starts_above_its_entire_header() {
        Sources.pages={"fixture native:story:one":doc([
            {type:"heading",level:2,text:"The first section"},
            {type:"paragraph",text:"A short article. ".repeat(40)},
            {type:"quote",text:"A short quotation.",attribution:"An author"},
            {type:"list",ordered:false,items:["First","Second"]}
        ],{image:"https://example.test/cover.jpg",image_width:1200,image_height:700})}
        const page=createTemporaryObject(pageFactory,test)
        tryCompare(page,"count",7)
        wait(100)
        verify(Math.abs(page.contentY-(page.headerItem.y-page.topMargin))<1)
    }
    function test_large_documents_mount_only_the_visible_layout_units() {
        const blocks=[]
        for(let i=0;i<200;i++)blocks.push({type:"paragraph",text:("Paragraph "+i+". ").repeat(100)})
        Sources.pages={"fixture native:story:one":doc(blocks)}
        const page=createTemporaryObject(pageFactory,test)
        tryCompare(page,"count",201)
        wait(50)
        verify(descendants(page,node=>node.objectName==="articleBlockText").length<30)
        page.positionViewAtIndex(180,ListView.Beginning);wait(50)
        verify(descendants(page,node=>node.objectName==="articleBlockText").length<30)
    }
    function test_text_chunks_preserve_surrogate_pairs_and_lists_stay_plain() {
        const text="a".repeat(8191)+"\ud840\udc00"+"tail"
        const projection=createTemporaryObject(projectionFactory,test,{article:{blocks:[{type:"paragraph",text:text},{type:"list",ordered:true,items:["<b>First</b>","Second"]}]}})
        compare(projection.rows.length,4)
        compare(projection.rows[0].text+projection.rows[1].text,text)
        compare(projection.rows[0].text.length,8191)
        compare(projection.rows[2].text,"<b>First</b>");compare(projection.rows[2].marker,"1.")
        compare(projection.rows[3].marker,"2.")
    }
    function test_context_invalidation_clears_text_and_image_demands() {
        const page=createTemporaryObject(pageFactory,test)
        const model=findChild(page,"nativeArticleModel")
        tryCompare(page,"count",2)
        model.invalidate()
        tryCompare(page,"count",0);compare(Object.keys(model.leases).length,0)
        const calls=model.pollCount
        wait(1100);compare(model.pollCount,calls)
    }
    function test_a_late_layout_callback_does_not_move_an_outgoing_article() {
        const blocks=[]
        for(let i=0;i<20;i++)blocks.push({type:"paragraph",text:"A longer paragraph. ".repeat(20)})
        Sources.pages={"fixture native:story:one":doc(blocks)}
        const page=createTemporaryObject(pageFactory,test)
        const model=findChild(page,"nativeArticleModel")
        tryCompare(page,"count",21);wait(50)
        const updated=JSON.parse(model.document)
        updated.title="A changed title"
        model.document=JSON.stringify(updated)
        page.leaving=true;page.contentY=120
        wait(50)
        compare(page.contentY,120)
    }
    function test_a_stale_offline_copy_keeps_its_body_and_shows_a_plain_note() {
        const saved=JSON.parse(doc())
        saved.offline=true;saved.stale=true;saved.error="A plain <b>cache note</b>"
        Sources.pages={"fixture native:story:one":JSON.stringify(saved)}
        const page=createTemporaryObject(pageFactory,test)
        tryCompare(page,"count",2)
        const note=findChild(page,"articleCacheNote")
        verify(note);compare(note.visible,true);compare(note.text,saved.error);compare(note.textFormat,Text.PlainText)
        compare(findChild(page,"nativeArticleModel").error,"")
    }
}
