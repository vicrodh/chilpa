use strict;
use warnings;
use Test::More;
use Chilpa::Contributions;
use Chilpa::Playback;

local $Chilpa::Contributions::ACTIVE = Chilpa::Contributions->new;
my ($pending, $song);
{
    package Guest::Handler;
    sub getNextTrack { ($song, $pending) = @_[1, 2] }
}
Slim::Player::ProtocolHandlers->registerHandler('guest', 'Guest::Handler');
my $result;
Chilpa::Playback->resolve({id => 'guest://recording:17'}, sub { $result = shift });
ok(!defined $result, 'resolution waits for the original asynchronous handler');
is($song->currentTrack->url, 'guest://recording:17', 'handler receives the persistent media identity');
$song->pluginData(mime => 'audio/flac');
$song->pluginData(samplesize => 24);
$song->pluginData(samplerate => 96000);
is($song->pluginData('mime'), 'audio/flac', 'per-song metadata stays available to the handler');
$song->streamUrl('https://media.example.org/recording');
$pending->();
is($result->{codec}, 'flac', 'an extensionless stream carries its decoder format hint');
is($result->{url}, 'https://media.example.org/recording', 'resolved URL becomes the shared stream response');
is($result->{quality}{sample_rate_hz}, 96000, 'shared stream contract retains the source-declared sample rate');
ok(!exists($result->{measured_quality}), 'plugin hints are not promoted to measured audio quality');
is_deeply($result->{selected_quality}, {format => 'audio/flac', sample_rate_hz => 96000, bit_depth => 24},
    'selected stream hints are retained separately from measured quality');
eval { Chilpa::Playback->resolve({id => 'file:///private'}, sub {}) };
like($@, qr/invalid resolved stream/, 'local files cannot become playback URLs');
eval { Chilpa::Playback->resolve({id => 'https://user:secret@example.org/media'}, sub {}) };
like($@, qr/invalid resolved stream/, 'URL credentials are rejected');
done_testing;
