package Chilpa::HostServices;

use strict;
use warnings;
use MIME::Base64 qw(encode_base64 decode_base64);
use Slim::Utils::Prefs;
use Slim::Networking::SimpleAsyncHTTP;
use Chilpa::Cookies;

sub attach {
    my ($class, $channel) = @_;
    $Slim::Utils::Prefs::LOAD = sub {
        $channel->service_sync({name => 'private_get', key => $_[0]});
    };
    $Slim::Utils::Prefs::SAVE = sub {
        $channel->service_sync({name => 'private_set', key => $_[0], value => $_[1]});
    };
    $Slim::Networking::Async::HTTP::COOKIES = Chilpa::Cookies->new(
        load => sub { $channel->service_sync({name => 'private_get', key => 'http:cookies'}) },
        save => sub { $channel->service_sync({name => 'private_set', key => 'http:cookies', value => $_[0]}) },
    );
    $Slim::Networking::SimpleAsyncHTTP::TRANSPORT = sub {
        my ($request, $callback) = @_;
        my %wire = %$request;
        $wire{body_base64} = encode_base64(delete($wire{body}) // '', '');
        $channel->service({name => 'http', request => \%wire}, sub {
            my ($response) = @_;
            return $callback->({error => 'host request failed'}) unless $response->{ok};
            my %reply = %{$response->{value}};
            $reply{body} = decode_base64(delete($reply{body_base64}) // '');
            $callback->(\%reply);
        });
    };
}

1;
