use strict;
use warnings;
use Test::More;
use Chilpa::Contributions;
use Chilpa::Metadata;

local $Chilpa::Contributions::ACTIVE = Chilpa::Contributions->new;
{
    package Guest::Metadata;
    sub getMetadataFor {
        return {title => 'Track', samplerate => 88200, samplesize => 24, ct => 'flac',
            token => 'private', url => 'https://example.org/signed', nested => {secret => 'private'}};
    }
    sub getNextTrack { die 'metadata must not select a stream' }
}
Slim::Player::ProtocolHandlers->registerHandler('guest', 'Guest::Metadata');
my $result;
Chilpa::Metadata->read({id => 'guest://track/1'}, sub { $result = shift });
ok($result->{available}, 'registered protocol metadata is available without playback');
is_deeply($result->{presentation}{advertised_quality}, {format => 'flac', sample_rate_hz => 88200,
    bit_depth => 24}, 'technical metadata survives the protocol interface');
unlike(JSON::PP->new->encode($result), qr/private|signed|nested/, 'raw metadata and private fields stay inside the worker');
Chilpa::Metadata->read({id => 'other://track/1'}, sub { $result = shift });
ok(!$result->{available}, 'unsupported metadata is explicit');
my $cached = Chilpa::Metadata->cached('guest://track/1');
is($cached->{samplerate}, 88200, 'cached metadata is available during listing');
{
    package Guest::Uncached;
    sub getMetadataFor { Slim::Networking::SimpleAsyncHTTP->new(sub {}, sub {})->get('https://example.org/metadata'); }
}
require Slim::Networking::SimpleAsyncHTTP;
Slim::Player::ProtocolHandlers->registerHandler('missing', 'Guest::Uncached');
my $requests = 0;
local $Slim::Networking::SimpleAsyncHTTP::TRANSPORT = sub { $requests++ };
is_deeply(Chilpa::Metadata->cached('missing://track/1'), {}, 'a metadata cache miss is optional');
is($requests, 0, 'listing enrichment never reaches the host network transport');
done_testing;
