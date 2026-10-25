//! I aggregate stored album facts in SQL without opening media or materializing tracks.
use super::Library;
use rusqlite::types::ValueRef;
use serde_json::{Value, json};

const FIELDS: &[&str] = &["format", "codec", "rate", "bits", "channels", "year", "source"];
fn query() -> String {
    let formats = ["flac", "wav", "wave", "aif", "aiff", "mp3", "ogg", "oga", "opus", "m4a", "mp4", "aac", "wma", "ape", "wv", "dsf", "dff", "iso"];
    let format = formats.iter().map(|ext| format!("WHEN lower(coalesce(sc.container,t.external_id)) LIKE '%.{ext}' THEN '{}'", ext.to_uppercase())).collect::<Vec<_>>().join(" ");
    let aggregates = FIELDS.iter().map(|field| format!("count(DISTINCT {field}), count({field}), min({field})")).collect::<Vec<_>>().join(",");
    format!("WITH members AS (
        SELECT CASE WHEN s.provider='local' THEN CASE {format} END END AS format,
            nullif(upper(trim(t.codec)), '') AS codec,
            CASE WHEN t.sample_rate_hz>0 THEN t.sample_rate_hz END AS rate,
            CASE WHEN lower(t.codec) IN ('dsd','dsf','dff','dst') THEN 1 WHEN t.bit_depth>0 THEN t.bit_depth END AS bits,
            CASE WHEN t.channels>0 THEN t.channels END AS channels,
            CASE WHEN coalesce(t.release_year,a.year)>0 THEN coalesce(t.release_year,a.year) END AS year,
            nullif(coalesce(nullif(s.label,''),s.provider),'') AS source,
            CASE WHEN t.duration_ms>0 THEN t.duration_ms END AS duration,
            CASE WHEN s.provider='local' THEN coalesce(sc.container,sc.path) END AS physical_path,
            CASE WHEN s.provider='local' AND physical.size>=0 THEN physical.size END AS file_bytes
        FROM tracks t INDEXED BY tracks_album
        JOIN albums a ON a.id=t.album_id JOIN sources s ON s.id=t.source_id
        LEFT JOIN scanned_files sc ON sc.path=t.external_id
        LEFT JOIN scanned_files physical ON physical.path=coalesce(sc.container,sc.path) AND physical.container IS NULL
        WHERE t.album_id=?1 AND t.present=1 AND s.enabled=1
    ) SELECT count(*), CASE WHEN count(duration)=count(*) AND count(*)>0 THEN sum(duration) END,
        {aggregates},
        (SELECT CASE WHEN count(*)>0 AND count(size)=count(*) THEN sum(size) END FROM
            (SELECT physical_path, min(file_bytes) AS size FROM members GROUP BY physical_path)),
        (SELECT {} FROM albums a WHERE a.id=?1)
        FROM members", album_art!("a"))
}
impl Library {
    /// Unknown members keep a scalar unknown; distinct known values are explicitly mixed.
    pub fn album_technical(&self, id: i64) -> rusqlite::Result<Value> {
        self.db.query_row(&query(), [id], |row| {
            let count: i64 = row.get(0)?;
            if count == 0 { return Ok(json!({})); }
            let mut document = json!({"tracks":count,"duration_ms":row.get::<_,Option<i64>>(1)?});
            let mut mixed = Vec::new();
            for (index, field) in FIELDS.iter().enumerate() {
                let offset = 2 + index * 3;
                let distinct: i64 = row.get(offset)?;
                let known: i64 = row.get(offset + 1)?;
                document[*field] = if distinct == 1 && known == count {
                    match row.get_ref(offset + 2)? {
                        ValueRef::Integer(number) => json!(number),
                        ValueRef::Text(text) => json!(String::from_utf8_lossy(text)),
                        _ => Value::Null,
                    }
                } else { Value::Null };
                if distinct > 1 { mixed.push(*field); }
            }
            document["mixed_fields"] = json!(mixed);
            document["bytes"] = json!(row.get::<_,Option<i64>>(2 + FIELDS.len() * 3)?);
            document["artwork"] = json!(row.get::<_,Option<String>>(3 + FIELDS.len() * 3)?);
            Ok(document)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Library {
        let lib = Library::in_memory().unwrap();
        lib.db.execute_batch("INSERT INTO sources(id,provider,instance,label) VALUES(1,'local','default','Local');
            INSERT INTO roots(id,source_id,path) VALUES(1,1,'/absent');
            INSERT INTO albums(id,title,album_artist,year,artwork_path) VALUES(1,'Album','Artist',2003,'/absent/cover.jpg');
            INSERT INTO tracks(id,source_id,external_id,title,album_id,duration_ms,sample_rate_hz,bit_depth,channels,codec,added_at)
            VALUES(1,1,'/absent/a.flac','One',1,1000,96000,24,2,'flac',0),
                  (2,1,'/absent/b.flac','Two',1,2000,96000,24,2,'flac',0);
            INSERT INTO scanned_files(root_id,path,mtime,size,track_id,scanned_at)
            VALUES(1,'/absent/a.flac',0,100,1,0),(1,'/absent/b.flac',0,200,2,0);").unwrap();
        lib
    }
    #[test]
    fn stored_multitrack_album_is_aggregated_without_opening_absent_media() {
        let lib = fixture();
        let before = lib.db.total_changes();
        let facts = lib.album_technical(1).unwrap();
        assert_eq!(facts, json!({"tracks":2,"duration_ms":3000,"format":"FLAC","codec":"FLAC","rate":96000,
            "bits":24,"channels":2,"bytes":300,"year":2003,"source":"Local","artwork":"/absent/cover.jpg","mixed_fields":[]}));
        assert_eq!(lib.db.total_changes(), before, "the projection writes nothing");
        let plan: Vec<String> = lib.db.prepare(&format!("EXPLAIN QUERY PLAN {}", query())).unwrap()
            .query_map([1], |row| row.get(3)).unwrap().collect::<Result<_,_>>().unwrap();
        assert!(plan.iter().any(|step| step.contains("tracks_album") && step.contains("album_id=?")), "album membership uses its index: {plan:?}");
    }
    #[test]
    fn heterogeneous_and_partly_unknown_values_never_claim_a_best_track() {
        let lib = fixture();
        lib.db.execute_batch("UPDATE tracks SET codec='mp3',external_id='/absent/b.mp3',sample_rate_hz=44100,bit_depth=NULL,channels=NULL,release_year=2004,duration_ms=0 WHERE id=2").unwrap();
        let facts = lib.album_technical(1).unwrap();
        assert_eq!(facts["mixed_fields"], json!(["format","codec","rate","year"]));
        for field in ["format","codec","rate","bits","channels","year","duration_ms","bytes"] { assert!(facts[field].is_null(), "{field}"); }
    }
    #[test]
    fn shared_container_sizes_need_independent_physical_evidence_and_are_counted_once() {
        let lib = fixture();
        lib.db.execute_batch("UPDATE scanned_files SET container='/absent/disc.iso',size=123456;
            UPDATE tracks SET codec='dsd',sample_rate_hz=2822400,bit_depth=NULL;").unwrap();
        let unknown = lib.album_technical(1).unwrap();
        assert_eq!(unknown["format"], "ISO"); assert_eq!(unknown["bits"], 1);
        assert!(unknown["bytes"].is_null(), "container rows hold a change stamp, not trustworthy byte length");
        lib.db.execute("INSERT INTO scanned_files(root_id,path,mtime,size,scanned_at) VALUES(1,'/absent/disc.iso',0,5000,0)", []).unwrap();
        assert_eq!(lib.album_technical(1).unwrap()["bytes"], 5000, "one physical image, two logical tracks");
    }
}
