// I interleave integrated origins by name and keep each separate source together.
function rows(catalog, order, ascending, query, expanded, groupTitle) {
    const playlists = (catalog.playlists || []).slice(), folders = catalog.folders || []
    const native = catalog.native_folders || [], providers = catalog.folder_sources || []
    const direction = ascending ? 1 : -1, result = [], visited = new Set()
    const sourceIds = [...new Set(providers.map(p => p.id).concat(playlists.map(p => p.source), native.map(f => f.source)))].filter(Boolean)
    function provider(source) { return providers.find(p => p.id === source) || ({}) }
    function separate(source) {
        if (!source) return false
        const info = provider(source)
        if (typeof info.separate === "boolean") return info.separate
        const rows = playlists.filter(p => p.source === source)
        return rows.length ? rows.some(p => p.separate) : true
    }
    function originRank(source) { return !source ? -1 : provider(source).position ?? sourceIds.indexOf(source) }
    function nameOrder(a, b) { return direction * a.name.localeCompare(b.name) || originRank(a.source) - originRank(b.source) || String(a.id || a.ref).localeCompare(String(b.id || b.ref)) }
    function playlistOrder(a, b) {
        return direction * (order === "recent" ? (b.updated_at || 0) - (a.updated_at || 0)
            : order === "tracks" ? (b.track_count || 0) - (a.track_count || 0)
            : order === "custom" ? (a.custom_position || 0) - (b.custom_position || 0) : a.name.localeCompare(b.name))
            || originRank(a.source) - originRank(b.source) || a.id - b.id
    }
    playlists.sort(playlistOrder)
    const base = {rowKind:1,playlistId:0,tracks:0,source:"",sourceName:"",local:false,artwork:"",visibleInSidebar:true,covers:[],folderColor:"",sourceIcon:"",showSourceBadge:false,depth:0,nativeRef:"",expandKey:""}
    function open(key) { return query !== "" || !!expanded[key] }
    function folder(value, key, children) {
        result.push(Object.assign({}, base, value, {id:"folder:" + key,expanded:open(key),expandKey:key}))
        if (open(key)) children()
    }
    function playlist(p, depth) {
        return {id:"playlist:" + (p.source || "") + ":" + p.id,rowKind:0,playlistId:p.id,name:p.name,tracks:p.track_count,source:p.source || "",sourceName:p.provider || "",local:p.mixed === true,
            sourceIcon:p.mixed ? "qrc:/qt/qml/app/chilpa/kit/assets/logo.png" : p.source_icon || "",showSourceBadge:true,artwork:p.artwork_path || "",
            folderId:p.folder_id || 0,visibleInSidebar:p.visible_sidebar !== false,covers:p.covers || [],folderColor:"",expanded:false,
            depth:depth,nativeRef:p.native_ref || "",nativeFolder:p.native_folder || "",containArt:p.contain_art === true}
    }
    let nextFolderId = -1000
    function branch(scope, parent, depth) {
        const chosen = playlists.filter(p => scope === null ? !separate(p.source) : p.source === scope)
        const entries = []
        if (!parent) {
            for (const f of folders) {
                const children = chosen.filter(p => p.folder_id === f.id)
                if (scope !== null && !children.length) continue
                if (query && !children.length && !f.name.toLowerCase().includes(query.toLowerCase())) continue
                entries.push({id:f.id,name:f.name,source:scope || "",localFolder:f,children:children})
            }
        }
        for (const f of native) {
            if ((f.parent || "") !== parent || (scope === null ? separate(f.source) : f.source !== scope)) continue
            entries.push({id:f.ref,name:f.name,source:f.source,nativeFolder:f})
        }
        entries.sort(order === "custom" ? (a,b) => direction * ((a.localFolder?.position || 0) - (b.localFolder?.position || 0)) || nameOrder(a,b) : nameOrder)
        for (const entry of entries) {
            if (entry.localFolder) {
                const f = entry.localFolder, key = scope === null ? f.id : scope + " local:" + f.id
                folder({name:f.name,source:scope || "",tracks:entry.children.length,local:true,folderId:f.id,folderColor:f.color || "",depth:depth,
                    sourceIcon:scope ? provider(scope).folder_icon || provider(scope).icon || "" : ""}, key,
                    () => entry.children.forEach(p => result.push(playlist(p, depth + 1))))
            } else {
                const f = entry.nativeFolder, key = f.source + " " + f.ref
                if (visited.has(key)) continue
                visited.add(key)
                folder({name:f.name,source:f.source,sourceName:provider(f.source).name || "",folderId:nextFolderId--,nativeRef:f.ref,editable:f.editable !== false,depth:depth,
                    tracks:chosen.filter(p => !p.folder_id && p.native_folder === f.ref).length,sourceIcon:f.folder_icon || f.source_icon || provider(f.source).icon || ""},key,
                    () => branch(f.source, f.ref, depth + 1))
            }
        }
        for (const p of chosen) {
            if (p.folder_id) continue
            const nativeParent = p.native_folder || ""
            const orphan = nativeParent && !native.some(f => f.source === p.source && f.ref === nativeParent)
            if (nativeParent === parent || !parent && orphan) result.push(playlist(p, depth))
        }
    }
    branch(null, "", 0)
    sourceIds.sort((a,b) => originRank(a) - originRank(b) || a.localeCompare(b))
    for (const source of sourceIds.filter(separate)) {
        if (!playlists.some(p => p.source === source) && !native.some(f => f.source === source)) continue
        const first = playlists.find(p => p.source === source), info = provider(source)
        result.push(Object.assign({}, base, {id:"source:" + source,rowKind:2,name:groupTitle || "Playlists",source:source,sourceName:info.name || first?.provider || source,
            sourceIcon:info.icon || first?.source_icon || "",folderId:nextFolderId--}))
        branch(source, "", 0)
    }
    return result
}
