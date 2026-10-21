//! I read record buckets once per query and plays from their existing index.

use super::{marks, Args, Field, Values};
use rusqlite::{functions::FunctionFlags, types::ToSql, Connection};

pub(crate) fn register(db: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    db.create_scalar_function("chilpa_release_kind", 4, flags, |ctx| {
        Ok(crate::release_kind::release_kind("", &ctx.get::<String>(0)?, &ctx.get::<String>(1)?,
            ctx.get::<i64>(2)? as usize, ctx.get(3)?).to_string())
    })?;
    db.create_scalar_function("chilpa_quality", 4, flags, |ctx| {
        Ok(crate::quality::Quality::of_track(&ctx.get::<String>(0)?, ctx.get::<Option<String>>(1)?.as_deref(),
            ctx.get(2)?, ctx.get(3)?).tier.rank())
    })
}

pub(super) fn records(field: Field, names: &[String], args: &mut Args) -> String {
    let predicate = match field {
        Field::Type => {
            args.extend(names.iter().map(|name| Box::new(name.clone()) as Box<dyn ToSql>));
            format!("SELECT a.id FROM albums a JOIN tracks f ON f.album_id = a.id AND f.present = 1
                GROUP BY a.id HAVING chilpa_release_kind(a.title, a.album_artist, count(*), coalesce(sum(f.duration_ms), 0)) IN ({})", marks(names.len()))
        }
        Field::Quality => {
            let ranks: Vec<i32> = names.iter().filter_map(|name| match name.as_str() {
                "lossy" => Some(1), "cd" => Some(2), "hires" => Some(3), "dsd" => Some(4), _ => None,
            }).collect();
            let sql = format!("SELECT album_id FROM tracks WHERE present = 1 AND album_id IS NOT NULL
                GROUP BY album_id HAVING max(chilpa_quality(external_id, codec, bit_depth, sample_rate_hz)) IN ({})", marks(ranks.len()));
            args.extend(ranks.into_iter().map(|rank| Box::new(rank) as Box<dyn ToSql>));
            sql
        }
        _ => unreachable!(),
    };
    format!("SELECT id FROM tracks WHERE album_id IN ({predicate})")
}

pub(super) fn plays(values: &Values, args: &mut Args) -> String {
    let (predicate, includes_zero) = match values {
        Values::Any(values) => {
            let counts: Vec<i64> = values.iter().filter_map(|v| v.parse().ok()).collect();
            let zero = counts.contains(&0);
            let predicate = format!("plays IN ({})", marks(counts.len()));
            args.extend(counts.into_iter().map(|n| Box::new(n) as Box<dyn ToSql>));
            (predicate, zero)
        }
        Values::Range(from, to) => {
            let mut parts = Vec::new();
            if let Some(from) = from { parts.push("plays >= ?"); args.push(Box::new(*from)); }
            if let Some(to) = to { parts.push("plays <= ?"); args.push(Box::new(*to)); }
            if parts.is_empty() { parts.push("1"); }
            (parts.join(" AND "), from.is_none_or(|v| v <= 0) && to.is_none_or(|v| v >= 0))
        }
        _ => unreachable!(),
    };
    let counted = format!("SELECT track_id FROM track_rollup INDEXED BY track_rollup_plays WHERE {predicate}");
    if includes_zero {
        format!("{counted} UNION SELECT id FROM tracks WHERE id NOT IN (SELECT track_id FROM track_rollup)")
    } else {
        counted
    }
}
