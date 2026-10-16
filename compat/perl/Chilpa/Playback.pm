package Chilpa::Playback;

use strict;
use warnings;
use URI;
use Slim::Player::ProtocolHandlers;
use Chilpa::Presentation;
use Chilpa::Client;

sub resolve {
    my ($class, $input, $done) = @_;
    my $url = $input->{id};
    die 'invalid media identity' unless defined($url) && !ref($url) && length($url) <= 16384
        && $url =~ /^[a-zA-Z][a-zA-Z0-9+.-]*:/;
    my $handler = Slim::Player::ProtocolHandlers->handlerForURL($url);
    my $song = Chilpa::Playback::Song->new($url);
    my $completed = 0;
    my $success = sub {
        return if $completed++;
        my $stream = $song->streamUrl;
        my $uri = URI->new($stream // '');
        die 'invalid resolved stream' unless ($uri->scheme // '') =~ /^https?$/ && $uri->host && !defined($uri->userinfo);
        # Metadata remains a hint. The audio pipeline measures the stream;
        # a plugin's declared bit depth is not proof of output quality.
        my %technical = (%{$song->pluginData}, %{$song->currentTrack});
        $technical{content_type} ||= $technical{mime};
        my $quality = Chilpa::Presentation::quality(\%technical);
        my $format = lc($technical{content_type} || $technical{format} || '');
        $format ||= lc($handler->getFormatForURL($url) || '') if $handler && $handler->can('getFormatForURL');
        my %formats = (flc=>'flac', flac=>'flac', 'audio/flac'=>'flac', 'audio/x-flac'=>'flac',
            mp3=>'mp3', 'audio/mpeg'=>'mp3', mp4=>'m4a', aac=>'aac', 'audio/aac'=>'aac', 'audio/mp4'=>'m4a', ogg=>'ogg', opus=>'opus');
        my $codec = $formats{$format};
        $done->({ url => $stream, urls => [], delivery => 'progressive', codec => $codec,
            selected_quality => $quality, quality => Chilpa::Presentation::shared_quality($quality) });
    };
    if ($handler) {
        die 'unsupported media resolver' unless $handler->can('getNextTrack');
        return $handler->getNextTrack($song, $success, sub {
            return if $completed++;
            $done->({error => 'The plugin could not resolve this recording.'});
        });
    }
    $song->streamUrl($url);
    $success->();
}

package Chilpa::Playback::Song;

sub new { bless { track => Chilpa::Playback::Track->new($_[1]), data => {} }, $_[0] }
sub currentTrack { $_[0]{track} }
sub track { $_[0]{track} }
sub master { Chilpa::Client->current }
sub streamUrl { $_[0]{stream} = $_[1] if @_ > 1; $_[0]{stream} }
sub bitrate { $_[0]->currentTrack->bitrate(@_[1 .. $#_]) }
sub samplerate { $_[0]->currentTrack->samplerate(@_[1 .. $#_]) }
sub samplesize { $_[0]->currentTrack->samplesize(@_[1 .. $#_]) }
sub pluginData {
    my ($self, $key, $value) = @_;
    return $self->{data} unless defined $key;
    $self->{data}{$key} = $value if @_ > 2;
    return $self->{data}{$key};
}

package Chilpa::Playback::Track;

sub new { bless { url => $_[1] }, $_[0] }
sub url { $_[0]{url} }
sub duration { $_[0]{duration} = $_[1] if @_ > 1; $_[0]{duration} }
sub bitrate { $_[0]{bitrate} = $_[1] if @_ > 1; $_[0]{bitrate} }
sub samplerate { $_[0]{samplerate} = $_[1] if @_ > 1; $_[0]{samplerate} }
sub samplesize { $_[0]{samplesize} = $_[1] if @_ > 1; $_[0]{samplesize} }
sub channels { $_[0]{channels} = $_[1] if @_ > 1; $_[0]{channels} }
sub content_type { $_[0]{content_type} = $_[1] if @_ > 1; $_[0]{content_type} }

1;
