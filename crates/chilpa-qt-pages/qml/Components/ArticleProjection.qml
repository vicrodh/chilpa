import QtQuick

// Wire blocks stay bounded; long plain text becomes small layout units.
QtObject {
    id: projection
    property var article: ({})
    readonly property var rows: project(article)
    function appendText(rows, type, value, extra) {
        const text=String(value || "")
        for(let start=0;start<text.length && rows.length<640;){
            let end=Math.min(start+8192,text.length)
            const last=text.charCodeAt(end-1)
            if(end<text.length && last>=0xd800 && last<=0xdbff)end--
            rows.push(Object.assign({type:type,text:text.slice(start,end),continuation:start>0},extra || {}))
            start=end
        }
    }
    function project(article) {
        const rows=[]
        appendText(rows,"summary",article.summary)
        let listed=0
        for(const block of (article.blocks || []).slice(0,256)){
            if(rows.length>=640)break
            if(block.type === "image"){
                rows.push(block)
                appendText(rows,"caption",block.caption)
            }else if(block.type === "list"){
                for(let index=0;index<(block.items || []).length && listed<256;index++,listed++)
                    appendText(rows,"listItem",block.items[index],{marker:block.ordered ? String(index+1)+"." : "\u2022"})
            }else if(["paragraph","heading","quote"].includes(block.type)){
                appendText(rows,block.type,block.text,{level:block.level || 2})
                if(block.type === "quote")appendText(rows,"attribution",block.attribution)
            }
        }
        return rows
    }
}
