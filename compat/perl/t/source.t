use strict;
use warnings;
use Test::More;
use Chilpa::Source;
use Chilpa::Contributions;

local $Chilpa::Contributions::ACTIVE = Chilpa::Contributions->new;
my $registry = $Chilpa::Contributions::ACTIVE;
my ($callback, $arguments, $context);
$registry->register('browse', 'Guest', {feed => sub {
    $_[1]->({items => [
        {name => 'Open', url => sub { ($callback, $arguments, $context) = @_[1..3] }, passthrough => [{collection => 42}]},
        {name => 'Track', type => 'audio', url => 'guest://recording:17', duration => 1.5},
        {name => 'Connect your account', type => 'textarea'},
    ]});
}});
my $source = Chilpa::Source->new;
my $listing;
$source->browse({}, sub { $listing = shift });
is(scalar @{$listing->{entries}}, 3, 'registered feed becomes a generic listing');
is($listing->{entries}[1]{id}, 'guest://recording:17', 'playable identity survives listing reorder and worker restart');
is($listing->{entries}[1]{duration_ms}, 1500, 'duration is converted to host units');
ok(!$listing->{entries}[2]{playable}, 'account instruction is not offered as playable audio');
my $handle = $listing->{entries}[0]{id};
$source->browse({at => $handle, cursor => '100', text => 'query'}, sub { $listing = shift });
is($arguments->{index}, 100, 'pagination reaches the original feed');
is($context->{collection}, 42, 'callback context remains private and unchanged');
$callback->({items => [{name => 'Next track', type => 'audio', url => 'guest://recording:18'}], offset => 100, total => 102});
is($listing->{next}, '101', 'source total drives the next cursor');
my $other = Chilpa::Source->new;
eval { $other->browse({at => $handle}, sub {}) };
like($@, qr/expired navigation/, 'a restarted worker refuses stale callback handles');
$registry->register('search', 'Guest', {func => sub {
    return {name => $_[1]{search}, items => [{name => 'Matches', type => 'search', url => sub {}}]};
}});
$source->browse({}, sub { $listing = shift });
is(scalar(grep { $_->{kind} eq 'search' && $_->{id} eq 'bridge:search' } @{$listing->{entries}}), 1, 'registered search is reachable even when the root only lists folders');
$source->browse({at => 'bridge:search', text => 'from field'}, sub { $listing = shift });
is($listing->{entries}[0]{title}, 'from field', 'source search field reaches the registered provider');
$source->search({text => 'needle'}, sub { $listing = shift });
is($listing->{entries}[0]{title}, 'needle', 'global search invokes the registered provider');
$source->browse({at => $listing->{entries}[0]{id}}, sub { $listing = shift });
is($listing->{entries}[0]{kind}, 'search', 'nested search controls remain searchable');
$registry->register('search', 'Guest', {func => sub {
    return {name => 'Results', items => [{name => 'Tracks', url => sub {
        $_[1]->({items => [
            {name => 'One', type => 'audio', url => 'guest://recording:17'},
            {name => 'Duplicate', type => 'audio', url => 'guest://recording:17'},
            {name => 'Two', type => 'audio', url => 'guest://recording:18'},
        ]});
    }}]};
}});
my $results;
$source->query({text => 'needle', limit => 1}, sub { $results = shift });
is(scalar @{$results->{tracks}}, 1, 'global search respects the requested result limit');
is($results->{tracks}[0]{external_id}, 'guest://recording:17', 'global search carries the original persistent identity');
$source->query({text => 'needle', limit => 10}, sub { $results = shift });
is(scalar @{$results->{tracks}}, 2, 'repeated tracks are deduplicated across search groups');

my $rich = $source->listing({items => [{name => 'Decorated title', line1 => 'Album', type => 'playlist',
    line2 => 'Artist', year => 2021, url => sub {}, favorites_url => 'guest://album:7',
    itemActions => {info => {command => ['guest-info'], fixedParams => {type => 'album', id => 7}}}},
    {name => 'Track', type => 'audio', url => 'guest://recording:20', artist => 'Artist', album => 'Album',
        tracknum => 3, media_number => 2, media_count => 2, work => 'Work', isrc => 'USRC17607839',
        content_type => 'flac', samplerate => 96000, samplesize => 24, bitrate => '2300 kbps'}]}, {});
my $album_display = $rich->{presentation}{$rich->{entries}[0]{id}};
is($album_display->{entity}, 'album', 'declared album identity survives a playlist interaction type');
is($album_display->{title}, 'Album', 'clean display title is preserved separately from decorated name');
is($album_display->{stable_id}, 'guest://album:7', 'persistent collection identity is retained separately from callback handle');
ok(!$album_display->{actions}[0]{enabled}, 'unsupported command is retained with an explicit unavailable state');
my $track_display = $rich->{presentation}{'guest://recording:20'};
is($track_display->{track_number}, 3, 'track number survives adaptation');
is($track_display->{disc_number}, 2, 'disc number survives adaptation');
is($track_display->{metadata}{work}, 'Work', 'classical work metadata survives adaptation');
is_deeply($track_display->{advertised_quality}, {format => 'flac', sample_rate_hz => 96000,
    bit_depth => 24, bitrate_label => '2300 kbps'}, 'technical fields and bitrate units survive adaptation');
is_deeply($album_display->{advertised_quality}, {}, 'a track does not establish quality for its album');
is_deeply(Chilpa::Presentation::quality({name => 'Hi-Res 24/192'}), {}, 'decorated names are not technical evidence');
my $static = $source->remember({items => [map { {name => "Track $_", type => 'audio', url => "guest://recording:$_"} } 1..250]});
$source->browse({at => $static, cursor => '100'}, sub { $listing = shift });
is(scalar @{$listing->{entries}}, 100, 'static collections are paged instead of silently truncated');
is($listing->{entries}[0]{id}, 'guest://recording:101', 'static pagination advances through the collection');
is($listing->{next}, '200', 'static pagination retains the total');
$registry->register('search', 'Guest', {func => sub { return {name => 'Results', items => [
    {name => 'Album', type => 'album', url => sub {die 'do not crawl album tracks in global search'}},
    {name => 'List', type => 'playlist', url => sub {}},
    {name => 'Song', type => 'audio', url => 'guest://recording:99', artist => 'Artist', album => 'Album'},
]}; }});
$source->query({text => 'needle', limit => 10}, sub { $results = shift });
is(scalar @{$results->{albums}}, 1, 'global search retains album results');
is(scalar @{$results->{playlists}}, 1, 'global search retains playlist results');
is($results->{tracks}[0]{artist}, 'Artist', 'global search retains explicit artist metadata');

my $implicit = $source->listing({items => [
    {name => 'Playable', url => 'guest://recording:100', play => 'guest://recording:100'},
    {name => 'Record', type => 'playlist', url => sub {}, passthrough => [{album_id => 7}]},
    {name => 'Performer', url => sub {}, passthrough => [{artistId => 8}]},
    {name => 'Web link', url => 'https://example.org/help'},
]}, {});
ok($implicit->{entries}[0]{playable}, 'an explicit play URI works without an audio type annotation');
is($implicit->{presentation}{$implicit->{entries}[1]{id}}{entity}, 'album', 'generic album context refines a playlist menu');
is($implicit->{presentation}{$implicit->{entries}[2]{id}}{entity}, 'artist', 'generic artist context identifies an artist menu');
ok(!$implicit->{entries}[3]{playable}, 'an ordinary web link does not become playable audio');
is($implicit->{presentation}{$implicit->{entries}[1]{id}}{stable_id}, 'album:7', 'collection identity does not depend on expiring menu handles');
my $paged_root = $source->listing({items => [{name => 'Record', url => sub {}}], offset => 0, total => 4}, {});
is($paged_root->{next}, '1', 'an injected search control does not advance the remote page cursor');
my $numeric = $source->listing({title => 2023, items => [{name => 7, line2 => 42, description => 1, type => 'text'}]}, {at => 'record'});
my $encoded = JSON::PP::encode_json($numeric);
like($encoded, qr/"title":"7"/, 'numeric text labels serialize as strings');
like($encoded, qr/"subtitle":"42"/, 'numeric subtitles serialize as strings');
like($encoded, qr/"description":"1"/, 'numeric descriptions serialize as strings');
like($encoded, qr/"title":"2023"/, 'numeric listing titles serialize as strings');
{
    package Guest::CachedMetadata;
    sub getMetadataFor { return {duration => 210, artist => 'Performer', album => 'Record', ct => 'flac', samplesize => 24, samplerate => 88200} }
}
Slim::Player::ProtocolHandlers->registerHandler('cached', 'Guest::CachedMetadata');
my $enriched = $source->listing({items => [{type => 'audio', url => 'cached://track/1', name => 'Recording'}]}, {at => 'record'});
is($enriched->{entries}[0]{duration_ms}, 210000, 'duration comes from the protocol metadata cache');
is($enriched->{presentation}{'cached://track/1'}{metadata}{artist}, 'Performer', 'structured artist comes from cached metadata');
is($enriched->{presentation}{'cached://track/1'}{advertised_quality}{sample_rate_hz}, 88200, 'cached quality keeps its reported units');
{
    my $keeper = Chilpa::Source->new;
    my $record = sub { {name => 'Record', type => 'playlist', url => sub {}, passthrough => [{album_id => 'r1'}]} };
    my $first = $keeper->listing({items => [$record->(), {name => 'Menu', url => sub {}}]}, {});
    my $again = $keeper->listing({items => [$record->(), {name => 'Menu', url => sub {}}]}, {});
    is($again->{entries}[0]{id}, $first->{entries}[0]{id}, 'a record listed again keeps its handle');
    isnt($again->{entries}[1]{id}, $first->{entries}[1]{id}, 'a folder without identity gets a new one');
    ok($keeper->{nodes}{$first->{entries}[0]{id}}, 'the kept handle still opens the record');
    my $artist = {name => 'Band', url => sub {}, passthrough => [{artist_id => 'a1'}]};
    my $page = $keeper->listing({items => [{name => 'Albums', url => sub {}, passthrough => [{artist_id => 'a1'}]},
        {name => 'Similar', url => sub {}, passthrough => [{artist_id => 'a2'}]}]}, {}, 'Band', $artist);
    is($page->{presentation}{$page->{entries}[0]{id}}{entity}, 'folder', 'a part of an artist page is not the artist again');
    is($page->{presentation}{$page->{entries}[1]{id}}{entity}, 'artist', 'another artist listed there still is one');
    my $art = $keeper->listing({items => [{name => 'Cover', url => sub {}, icon => 'https://img.example/c.jpg'}]}, {});
    is($art->{entries}[0]{image_url}, 'https://img.example/c.jpg', 'an icon serves as the picture when there is no image');
}

done_testing;

