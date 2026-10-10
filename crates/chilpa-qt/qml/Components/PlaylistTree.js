function rows(catalog, order, ascending, query, expanded) {
    const playlists = (catalog.playlists || []).slice()
    const folders = (catalog.folders || []).slice()
    const native = catalog.native_folders || []
    const providers = catalog.folder_sources || []
    const direction = ascending ? 1 : -1
    playlists.sort((a,b) => direction * (order === "recent" ? b.updated_at - a.updated_at : order === "tracks" ? b.track_count - a.track_count : order === "custom" ? a.custom_position - b.custom_position : a.name.localeCompare(b.name)))
    folders.sort((a,b) => direction * (order === "custom" ? a.position - b.position : a.name.localeCompare(b.name)))
    const result = []
    const base = {rowKind:1,playlistId:0,tracks:0,source:"",sourceName:"",local:false,artwork:"",visibleInSidebar:true,covers:[],folderColor:"",sourceIcon:"",showSourceBadge:false,depth:0,nativeRef:"",expandKey:""}
    function open(key) { return query !== "" || !!expanded[key] }
    function folder(value, key, children) {
        result.push(Object.assign({},base,value,{expanded:open(key),expandKey:key}))
        if (open(key)) children()
    }
    function playlist(p,depth) {
        return {rowKind:0,playlistId:p.id,name:p.name,tracks:p.track_count,source:p.source,sourceName:p.provider,local:p.mixed,
            sourceIcon:p.source_icon || "",showSourceBadge:!p.mixed && (!p.separate || !!p.folder_id),artwork:p.artwork_path,
            folderId:p.folder_id || 0,visibleInSidebar:p.visible_sidebar,covers:p.covers,folderColor:"",expanded:false,
            depth:depth,nativeRef:p.native_ref || "",nativeFolder:p.native_folder || "",containArt:p.contain_art === true}
    }
    for (const f of folders) {
        const children = playlists.filter(p => p.folder_id === f.id)
        if (!query || children.length || f.name.toLowerCase().includes(query.toLowerCase()))
            folder({name:f.name,tracks:f.playlist_count,local:true,folderId:f.id,folderColor:f.color},f.id,() => children.forEach(p => result.push(playlist(p,1))))
    }
    for (const p of playlists) if (!p.folder_id && !p.separate && !p.native_folder) result.push(playlist(p,0))
    const sources = [...new Set(playlists.filter(p => p.separate && !p.folder_id).map(p => p.source).concat(native.map(f => f.source)))]
    let folderId = -1000
    for (const source of sources) {
        const children = playlists.filter(p => p.source === source && !p.folder_id)
        const provider = providers.find(p => p.id === source)
        const first = children[0]
        const name = provider ? provider.name : first ? first.provider : source
        const icon = provider ? provider.icon : first ? first.source_icon : ""
        const separate = children.length === 0 || children.some(p => p.separate)
        const seen = new Set()
        function branch(parent,depth) {
            const subfolders = native.filter(f => f.source === source && (f.parent || "") === parent)
            subfolders.sort((a,b) => direction * a.name.localeCompare(b.name))
            for (const f of subfolders) {
                if (seen.has(f.ref)) continue
                seen.add(f.ref)
                const key = source + " " + f.ref
                folder({name:f.name,source:source,sourceName:name,folderId:folderId--,nativeRef:f.ref,depth:depth,
                        tracks:children.filter(p => p.native_folder === f.ref).length,sourceIcon:f.folder_icon || ""},key,() => branch(f.ref,depth+1))
            }
            for (const p of children) if ((p.native_folder || "") === parent && (parent || p.separate)) result.push(playlist(p,depth))
        }
        if (separate)
            folder({name:name,source:source,sourceName:name,tracks:children.length,folderId:folderId--,sourceIcon:icon},"source:"+source,() => branch("",1))
        else
            branch("",0)
    }
    return result
}
