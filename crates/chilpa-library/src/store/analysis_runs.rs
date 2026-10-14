//! I save analysis progress with each result, including interrupted passes.
use super::{Library, now_unix};
use rusqlite::params;

pub struct AnalysisRun<'a> { library: &'a Library, pub id: i64, finished: bool }

pub struct AnalysisRunSummary {
    pub id: i64,
    pub analyzer: String,
    pub version: i64,
    pub folder: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub state: String,
    pub total: usize,
    pub previous: usize,
    pub processed: usize,
    pub saved: usize,
    pub failed: usize,
    pub error: String,
    pub last: String,
}

impl Library {
    pub fn begin_analysis_run(&self, analyzer: &str, version: i64, folder: Option<&str>, total: usize) -> rusqlite::Result<AnalysisRun<'_>> {
        let table = if analyzer == super::mood::ANALYZER { "mood_features" } else { "audio_features" };
        let under = folder.map(|path| format!("{}/", path.trim_end_matches('/')));
        let previous: i64 = self.db.query_row(&format!("SELECT count(*) FROM {table} f
            JOIN tracks t ON t.id = f.track_id JOIN sources s ON s.id = t.source_id
            WHERE f.analyzer = ?1 AND f.version >= ?2 AND s.kind = 'local'
            AND t.present = 1 AND t.reachable = 1
            AND (?3 IS NULL OR substr(t.external_id, 1, length(?3)) = ?3)"),
            params![analyzer, version, under], |row| row.get(0))?;
        let transaction = self.db.unchecked_transaction()?;
        transaction.execute("UPDATE analysis_runs SET state = 'interrupted', finished_at = ?2
            WHERE analyzer = ?1 AND state = 'running'", params![analyzer, now_unix()])?;
        transaction.execute("INSERT INTO analysis_runs(analyzer, version, folder, started_at, total, previous)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![analyzer, version, folder, now_unix(), total as i64, previous])?;
        let id = transaction.last_insert_rowid();
        transaction.commit()?;
        Ok(AnalysisRun { library:self, id, finished:false })
    }

    pub fn analysis_runs(&self) -> rusqlite::Result<Vec<AnalysisRunSummary>> {
        let mut query = self.db.prepare("SELECT r.id, r.analyzer, r.version, r.folder, r.started_at,
            r.finished_at, r.state, r.total, r.previous, r.processed, r.saved, r.failed, r.error,
            COALESCE((SELECT t.title FROM analysis_results p JOIN tracks t ON t.id = p.track_id
                WHERE p.run_id = r.id ORDER BY p.analyzed_at DESC, p.rowid DESC LIMIT 1), '')
            FROM analysis_runs r ORDER BY r.id DESC LIMIT 50")?;
        let rows = query.query_map([], |row| Ok(AnalysisRunSummary { id:row.get(0)?, analyzer:row.get(1)?, version:row.get(2)?,
            folder:row.get(3)?, started_at:row.get(4)?, finished_at:row.get(5)?, state:row.get(6)?,
            total:row.get::<_,i64>(7)? as usize, previous:row.get::<_,i64>(8)? as usize,
            processed:row.get::<_,i64>(9)? as usize, saved:row.get::<_,i64>(10)? as usize,
            failed:row.get::<_,i64>(11)? as usize, error:row.get(12)?, last:row.get(13)? }))?;
        rows.collect()
    }
}

impl AnalysisRun<'_> {
    pub fn record(&self, track: i64, saved: bool, write: impl FnOnce() -> rusqlite::Result<()>) -> rusqlite::Result<()> {
        let transaction = self.library.db.unchecked_transaction()?;
        write()?;
        transaction.execute("INSERT INTO analysis_results(run_id, track_id, state, analyzed_at) VALUES (?1, ?2, ?3, ?4)",
            params![self.id, track, if saved { "saved" } else { "failed" }, now_unix()])?;
        transaction.execute("UPDATE analysis_runs SET processed = processed + 1,
            saved = saved + ?2, failed = failed + ?3 WHERE id = ?1 AND state = 'running'",
            params![self.id, i64::from(saved), i64::from(!saved)])?;
        transaction.commit()
    }

    pub fn finish(&mut self, state: &str, error: &str) -> rusqlite::Result<()> {
        if !matches!(state, "completed" | "cancelled" | "failed" | "interrupted") { return Err(rusqlite::Error::InvalidQuery); }
        self.library.db.execute("UPDATE analysis_runs SET state = ?2, finished_at = ?3, error = ?4 WHERE id = ?1",
            params![self.id, state, now_unix(), error])?;
        self.finished = true;
        Ok(())
    }
}

impl Drop for AnalysisRun<'_> {
    fn drop(&mut self) {
        if !self.finished { let _ = self.finish("interrupted", ""); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn results_and_progress_survive_reopening_without_a_manual_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.db");
        let id;
        {
            let library = Library::open(&path).unwrap();
            let source = library.source_id("local", "default").unwrap();
            library.db.execute("INSERT INTO tracks(source_id,external_id,title,added_at) VALUES (?1,'/track.flac','Saved track',0)",[source]).unwrap();
            let run = library.begin_analysis_run("chilpa",2,None,1).unwrap(); id = run.id;
            run.record(1,true,|| library.save_features(1,"chilpa",2,Some(120.0),None,None,None)).unwrap();
            // I leave the pass before its final completion event.
        }
        let library = Library::open(&path).unwrap();
        let runs = library.analysis_runs().unwrap();
        assert_eq!((runs[0].id,runs[0].processed,runs[0].saved), (id,1,1));
        assert_eq!(runs[0].state,"interrupted");
        assert_eq!(library.analysis_inventory(2,"standard","",0).unwrap().matched,1);
        assert!(library.tracks_pending_analysis("chilpa",2,10).unwrap().is_empty());
        let next = library.begin_analysis_run("chilpa",2,None,0).unwrap();
        assert_eq!(library.analysis_runs().unwrap()[0].previous,1);
        drop(next);
    }

    #[test]
    fn a_failed_commit_does_not_claim_an_analysis_was_saved() {
        let library = super::super::fixtures::sample_library();
        let mut run = library.begin_analysis_run("chilpa",2,None,3).unwrap();
        assert!(run.record(999,true,|| library.save_features(1,"chilpa",2,Some(120.0),None,None,None)).is_err());
        assert!(library.track_features(1,"chilpa").unwrap().is_none());
        run.record(1,false,|| Ok(())).unwrap();
        run.finish("cancelled","").unwrap();
        let history = library.analysis_runs().unwrap();
        assert_eq!((history[0].processed,history[0].saved,history[0].failed),(1,0,1));
        assert_eq!(history[0].state,"cancelled");
    }

    #[test]
    fn a_pass_keeps_previous_results_separate_from_its_new_results() {
        let library = super::super::fixtures::sample_library();
        library.save_mood_features(1,1,[0.5;6],&[0.0;super::super::mood::VECTOR_LENGTH]).unwrap();
        let mut run = library.begin_analysis_run(super::super::mood::ANALYZER,1,None,2).unwrap();
        run.record(2,true,|| library.save_mood_features(2,1,[0.7;6],&[0.0;super::super::mood::VECTOR_LENGTH])).unwrap();
        let page = library.analysis_inventory_for_run(2,"all","",0,run.id).unwrap();
        assert_eq!((page.total,page.mood,page.matched),(3,2,1));
        assert_eq!(page.tracks[0].id,2);
        assert_eq!(page.tracks[0].result,"saved");
        assert!(page.tracks[0].mood_at.is_some());
        let history = library.analysis_runs().unwrap();
        assert_eq!((history[0].previous,history[0].processed,history[0].saved),(1,1,1));
        run.finish("cancelled","").unwrap();
        assert_eq!(library.tracks_pending_mood_under(1,None).unwrap().len(),1);
    }
}
