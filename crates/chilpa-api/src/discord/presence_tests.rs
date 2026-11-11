use super::*;

type Calls = Arc<Mutex<Vec<Option<Value>>>>;

/// Writes down what Discord would be given: `Some` a status, `None` a clear.
struct Recorder {
    calls: Calls,
    reaches: bool,
}

impl Rpc for Recorder {
    fn show(&mut self, activity: Value) -> bool {
        self.calls.lock().unwrap().push(Some(activity));
        self.reaches
    }
    fn clear(&mut self) {
        self.calls.lock().unwrap().push(None);
    }
    fn close(&mut self) {}
}

struct Bench {
    presenter: Presenter<Recorder>,
    calls: Calls,
    searches: Arc<Mutex<Vec<String>>>,
    start: Instant,
}

impl Bench {
    fn new(read_tags: fn(&str) -> Tags, reaches: bool) -> Bench {
        let calls = Calls::default();
        let searches = Arc::new(Mutex::new(Vec::new()));
        let asked = searches.clone();
        let look_up: LookUp = Box::new(move |key, _| asked.lock().unwrap().push(key));
        let presenter = Presenter::new(Recorder { calls: calls.clone(), reaches }, read_tags, look_up, Arc::default());
        Bench { presenter, calls, searches, start: Instant::now() }
    }

    fn at(&self, ms: u64) -> Instant {
        self.start + Duration::from_millis(ms)
    }

    /// Hands a message in at `ms` and lets what is due by then happen.
    fn tell(&mut self, ms: u64, message: Message) {
        self.presenter.receive(message, self.at(ms));
        self.presenter.tick(self.at(ms));
    }

    fn wait(&mut self, ms: u64) {
        self.presenter.tick(self.at(ms));
    }

    fn calls(&self) -> Vec<Option<Value>> {
        self.calls.lock().unwrap().clone()
    }

    fn last(&self) -> Value {
        self.calls().last().cloned().flatten().expect("a status was shown")
    }

    fn searches(&self) -> Vec<String> {
        self.searches.lock().unwrap().clone()
    }
}

fn no_tags(_: &str) -> Tags {
    Tags::default()
}

fn defaults() -> Options {
    Options { show_state: true, clear_on_pause: false, title: "%title%".into(), artist: "%artist%".into(), album: "%album%".into(), artwork: true }
}

fn playing(path: &str, album: &str) -> Box<Playing> {
    Box::new(Playing {
        path: path.into(),
        title: "Time".into(),
        artist: "Pink Floyd".into(),
        album: album.into(),
        duration: 413.0,
        started_at: 1_000,
        look_up: true,
        ..Playing::default()
    })
}

fn show(path: &str, options: &Arc<Options>) -> Message {
    Message::Show(playing(path, "The Dark Side of the Moon"), options.clone())
}

const MOON: &str = "name:pink floyd\u{1f}the dark side of the moon";
const COVER: &str = "https://coverartarchive.org/release-group/f5093c06-23e3-404f-aeaa-40f72885ee3a/front-1200";

#[test]
fn a_playing_track_gives_every_field_of_the_status() {
    let mut bench = Bench::new(no_tags, true);
    let options = Arc::new(defaults());
    bench.tell(0, show("/music/time.flac", &options));
    assert!(bench.calls().is_empty(), "nothing before the changes settle");
    bench.wait(50);
    assert_eq!(bench.calls(), [Some(json!({
        "type": 2,
        "instance": false,
        "state": "Pink Floyd",
        "details": "Time",
        "timestamps": {"start": 1_000, "end": 1_413},
        "assets": {"large_image": "cover", "large_text": "The Dark Side of the Moon", "small_image": "play", "small_text": "Playing"},
        "buttons": [{"label": "Get Chilpa", "url": "https://chilpa.app"}],
    }))]);
}

#[test]
fn a_pause_drops_the_timestamps_and_says_so() {
    let mut bench = Bench::new(no_tags, true);
    let options = Arc::new(defaults());
    let mut paused = playing("/music/time.flac", "The Dark Side of the Moon");
    paused.paused = true;
    bench.tell(0, Message::Show(paused.clone(), options));
    bench.wait(50);
    assert_eq!(bench.last()["timestamps"], json!({}));
    assert_eq!(bench.last()["assets"]["small_text"], "Paused");
    assert_eq!(bench.last()["assets"]["small_image"], "pause");
    let options = Arc::new(Options { clear_on_pause: true, show_state: false, ..defaults() });
    bench.tell(100, Message::Show(paused, options.clone()));
    bench.wait(150);
    assert_eq!(bench.calls().last(), Some(&None), "cleared on pause");
    bench.tell(200, show("/music/time.flac", &options));
    bench.wait(250);
    assert!(bench.last()["assets"].get("small_image").is_none(), "no badge without the play state");
    assert!(bench.last()["assets"].get("small_text").is_none());
}

#[test]
fn changes_that_come_together_go_as_one_and_nothing_new_is_not_told() {
    let mut bench = Bench::new(no_tags, true);
    let options = Arc::new(defaults());
    bench.tell(0, show("/music/a.flac", &options));
    let mut later = playing("/music/b.flac", "Wish You Were Here");
    later.title = "Have a Cigar".into();
    bench.tell(30, Message::Show(later, options.clone()));
    bench.wait(50);
    assert_eq!(bench.calls().len(), 1);
    assert_eq!(bench.last()["details"], "Have a Cigar", "the last of them");
    bench.tell(500, Message::Show(Box::new(Playing { path: "/music/b.flac".into(), title: "Have a Cigar".into(), artist: "Pink Floyd".into(),
        album: "Wish You Were Here".into(), duration: 413.0, started_at: 1_000, look_up: true, ..Playing::default() }), options));
    bench.wait(550);
    assert_eq!(bench.calls().len(), 1, "the same status is not given again");
}

#[test]
fn stopping_clears_the_status() {
    let mut bench = Bench::new(no_tags, true);
    let options = Arc::new(defaults());
    bench.tell(0, show("/music/time.flac", &options));
    bench.wait(50);
    bench.tell(60, Message::Clear);
    bench.wait(110);
    assert_eq!(bench.calls().len(), 2);
    assert_eq!(bench.calls()[1], None);
}

#[test]
fn a_status_that_did_not_reach_discord_is_given_again_at_the_next_change() {
    let mut bench = Bench::new(no_tags, false);
    let options = Arc::new(defaults());
    bench.tell(0, show("/music/time.flac", &options));
    bench.wait(50);
    bench.tell(60, show("/music/time.flac", &options));
    bench.wait(110);
    assert_eq!(bench.calls().len(), 2, "tried again, as Fooyin connects on each update");
}

#[test]
fn a_cover_is_looked_for_a_second_after_the_track_changes_and_shown_when_found() {
    let mut bench = Bench::new(no_tags, true);
    let options = Arc::new(defaults());
    bench.tell(0, show("/music/time.flac", &options));
    bench.wait(999);
    assert!(bench.searches().is_empty(), "not before a second");
    assert_eq!(bench.presenter.next_wake(), Some(bench.at(1_000)));
    bench.wait(1_000);
    assert_eq!(bench.searches(), [MOON]);
    assert_eq!(bench.presenter.wanted.lock().unwrap().as_deref(), Some(MOON));
    assert_eq!(bench.last()["assets"]["large_image"], "cover", "the asset while it is looked for");
    bench.tell(1_500, Message::Cover(MOON.into(), Outcome::Found(COVER.into())));
    bench.wait(1_550);
    assert_eq!(bench.last()["assets"]["large_image"], COVER);
    assert_eq!(bench.last()["assets"]["small_image"], "icon", "the logo over a cover, the play badge without one");
    bench.tell(2_000, show("/music/money.flac", &options));
    bench.wait(4_000);
    assert_eq!(bench.searches().len(), 1, "another track of the album asks nothing");
    assert_eq!(bench.last()["assets"]["large_image"], COVER);
}

#[test]
fn skipping_through_tracks_looks_for_the_cover_of_the_one_that_stays() {
    let mut bench = Bench::new(no_tags, true);
    let options = Arc::new(defaults());
    bench.tell(0, show("/music/time.flac", &options));
    bench.tell(500, Message::Show(playing("/music/shine.flac", "Wish You Were Here"), options.clone()));
    bench.wait(1_000);
    assert!(bench.searches().is_empty());
    bench.wait(1_500);
    assert_eq!(bench.searches(), ["name:pink floyd\u{1f}wish you were here"]);
}

#[test]
fn a_search_still_running_for_another_album_is_waited_for() {
    let mut bench = Bench::new(no_tags, true);
    let options = Arc::new(defaults());
    bench.tell(0, show("/music/time.flac", &options));
    bench.wait(1_000);
    bench.tell(1_200, Message::Show(playing("/music/shine.flac", "Wish You Were Here"), options.clone()));
    assert_eq!(bench.presenter.wanted.lock().unwrap().as_deref(), Some("name:pink floyd\u{1f}wish you were here"), "the first one stops");
    bench.wait(2_500);
    assert_eq!(bench.searches().len(), 1, "one search at a time");
    bench.tell(2_600, Message::Cover(MOON.into(), Outcome::Abandoned));
    assert_eq!(bench.searches().len(), 2, "then the album that plays");
    bench.tell(2_650, Message::Cover("name:pink floyd\u{1f}wish you were here".into(), Outcome::Abandoned));
    bench.tell(2_700, show("/music/time.flac", &options));
    bench.wait(3_800);
    assert_eq!(bench.searches().len(), 3, "an abandoned search taught nothing");
}

#[test]
fn an_album_with_no_cover_is_not_asked_again_for_an_hour() {
    let mut bench = Bench::new(no_tags, true);
    let options = Arc::new(defaults());
    bench.tell(0, show("/music/time.flac", &options));
    bench.wait(1_000);
    bench.tell(1_100, Message::Cover(MOON.into(), Outcome::Missing));
    bench.tell(2_000, Message::Clear);
    bench.tell(3_000, show("/music/money.flac", &options));
    bench.wait(5_000);
    assert_eq!(bench.searches().len(), 1);
    bench.tell(3_600_000 + 1_200, Message::Clear);
    bench.tell(3_600_000 + 1_300, show("/music/money.flac", &options));
    bench.wait(3_600_000 + 2_400);
    assert_eq!(bench.searches().len(), 2, "after an hour it is asked again");
}

#[test]
fn a_sources_cover_is_shown_at_once_and_artwork_off_shows_the_asset() {
    let mut bench = Bench::new(no_tags, true);
    let options = Arc::new(defaults());
    let mut streamed = playing("source://plugin/1", "The Wall");
    streamed.cover_url = Some("https://images.example/wall.jpg".into());
    bench.tell(0, Message::Show(streamed.clone(), options));
    bench.wait(50);
    assert_eq!(bench.last()["assets"]["large_image"], "https://images.example/wall.jpg");
    let off = Arc::new(Options { artwork: false, ..defaults() });
    bench.tell(100, Message::Show(streamed, off.clone()));
    bench.tell(200, show("/music/time.flac", &off));
    bench.wait(5_000);
    assert_eq!(bench.last()["assets"]["large_image"], "cover");
    assert!(bench.searches().is_empty(), "nothing is looked for");
}

#[test]
fn a_track_that_may_not_be_looked_up_asks_nothing() {
    let mut bench = Bench::new(no_tags, true);
    let mut local = playing("/music/time.flac", "The Dark Side of the Moon");
    local.look_up = false;
    bench.tell(0, Message::Show(local, Arc::new(defaults())));
    bench.wait(5_000);
    assert!(bench.searches().is_empty(), "MusicBrainz is off");
}

fn moon_tags(path: &str) -> Tags {
    assert_eq!(path, "/music/time.flac");
    Tags {
        album_artist: "Pink Floyd".into(),
        date: "1973-03-01".into(),
        genres: vec!["Progressive rock".into(), "Psychedelia".into()],
        track: Some(4),
        release: "956FBC58-362D-43B8-B880-3779E0508559".into(),
        ..Tags::default()
    }
}

#[test]
fn the_formats_read_the_files_tags_and_its_ids_name_the_cover() {
    let mut bench = Bench::new(moon_tags, true);
    let options = Arc::new(Options { title: "%track%. %title%".into(), artist: "%albumartist%[ · %genre%]".into(),
        album: "%album% (%year%)".into(), ..defaults() });
    bench.tell(0, show("/music/time.flac", &options));
    bench.wait(1_000);
    assert_eq!(bench.last()["details"], "4. Time");
    assert_eq!(bench.last()["state"], "Pink Floyd · Progressive rock, Psychedelia");
    assert_eq!(bench.last()["assets"]["large_text"], "The Dark Side of the Moon (1973)");
    assert_eq!(bench.searches(), ["mbid:956fbc58-362d-43b8-b880-3779e0508559:"]);
}

#[test]
fn a_line_fits_what_discord_takes() {
    let mut bench = Bench::new(no_tags, true);
    let mut long = playing("/music/a.flac", "x");
    long.title = "ab".repeat(100);
    long.artist = String::new();
    bench.tell(0, Message::Show(long, Arc::new(Options { title: "%title%".into(), ..defaults() })));
    bench.wait(50);
    let details = bench.last()["details"].as_str().unwrap().to_string();
    assert_eq!(details.chars().count(), 128);
    assert!(details.ends_with('…'));
    assert_eq!(bench.last()["state"], "  ", "an empty line is padded, as Fooyin pads it");
    assert_eq!(bench.last()["assets"]["large_text"], "x  ");
}

#[test]
fn an_unreadable_format_says_nothing() {
    let mut bench = Bench::new(no_tags, true);
    bench.tell(0, show("/music/a.flac", &Arc::new(Options { title: "$if(%title%".into(), ..defaults() })));
    bench.wait(50);
    assert_eq!(bench.last()["details"], "  ");
}

#[test]
fn a_duration_reads_in_minutes_and_seconds() {
    assert_eq!(clock(413.4), "06:53");
    assert_eq!(clock(3_725.0), "01:02:05");
    assert_eq!(year("1973-03-01"), "1973");
    assert_eq!(year("Spring 73"), "");
}

#[cfg(unix)]
mod discord {
    use super::super::*;
    use std::io::{Read, Write};

    static PIPE: Mutex<Option<PathBuf>> = Mutex::new(None);

    fn pipe() -> Vec<PathBuf> {
        PIPE.lock().unwrap().clone().into_iter().collect()
    }

    fn frame(stream: &mut impl Read) -> Option<(u32, Value)> {
        let mut header = [0u8; 8];
        stream.read_exact(&mut header).ok()?;
        let mut body = vec![0u8; u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize];
        stream.read_exact(&mut body).ok()?;
        Some((u32::from_le_bytes(header[0..4].try_into().unwrap()), serde_json::from_slice(&body).unwrap()))
    }

    fn answer(stream: &mut impl Write, payload: Value) {
        let body = payload.to_string();
        let mut out = 1u32.to_le_bytes().to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body.as_bytes());
        stream.write_all(&out).unwrap();
    }

    #[test]
    fn discord_is_tried_at_each_change_and_found_once_it_runs() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("discord-ipc-0");
        *PIPE.lock().unwrap() = Some(path.clone());
        let mut discord = Discord { pipes: pipe, ..Discord::default() };
        assert!(!discord.show(json!({"details": "Time"})), "Discord is not running");
        discord.clear();
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let heard = std::thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            let mut heard = Vec::new();
            while let Some((opcode, payload)) = frame(&mut peer) {
                match opcode {
                    0 => answer(&mut peer, json!({"cmd": "DISPATCH", "evt": "READY"})),
                    1 => answer(&mut peer, json!({"cmd": "SET_ACTIVITY", "nonce": payload["nonce"]})),
                    _ => break,
                }
                heard.push(payload);
            }
            heard
        });
        assert!(discord.show(json!({"details": "Time"})), "the next change finds it");
        discord.clear();
        discord.close();
        let heard = heard.join().unwrap();
        assert_eq!(heard[0]["client_id"], APPLICATION_ID, "Chilpa's own application");
        assert_eq!(heard[1]["args"]["activity"]["details"], "Time");
        assert_eq!(heard[2]["args"]["activity"], Value::Null);
        assert_eq!(heard.len(), 3);
    }
}
