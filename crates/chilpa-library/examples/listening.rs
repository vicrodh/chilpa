//! I inspect listening queries on an explicitly supplied disposable library copy.
use chilpa_library::{store::listening::*, Library};
use std::{path::Path, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("provide a disposable library copy; opening it may migrate it")?;
    let lib = Library::open(Path::new(&path))?;
    let filter = ListenFilter::default();
    println!("integrity: {:?}", lib.check()?);
    println!("legacy: {:?}", lib.stats()?);
    println!("summary: {:?}", lib.listening_summary(&filter)?);
    for dimension in [
        ListenDimension::Genre,
        ListenDimension::Hour,
        ListenDimension::GenreHour,
        ListenDimension::Artist,
        ListenDimension::Album,
    ] {
        let start = Instant::now();
        let rows = lib.listening_buckets(&filter, dimension, ListenOrder::ListenedTime, 10)?;
        println!("{dimension:?}: {rows:?} ({:?})", start.elapsed());
    }
    Ok(())
}
