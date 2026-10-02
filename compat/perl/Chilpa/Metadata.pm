package Chilpa::Metadata;

use strict;
use warnings;
use Chilpa::Presentation;
use Chilpa::Client;
use Slim::Player::ProtocolHandlers;

# A deliberate, on-demand metadata request. Never resolve every track while
# listing an album: resolution can select a stream or consume a signed URL.
# Menu enrichment reads cached protocol metadata only. A cache miss must not
# make a separate network request for every visible track or resolve streams.
sub cached {
    my ($class, $id) = @_;
    my $handler = Slim::Player::ProtocolHandlers->handlerForURL($id);
    return {} unless $handler && $handler->can('getMetadataFor');
    local $Slim::Networking::SimpleAsyncHTTP::TRANSPORT;
    my $metadata = eval { $handler->getMetadataFor(undef, $id) };
    return {} unless ref($metadata) eq 'HASH';
    return { %$metadata, content_type => $metadata->{content_type} || $metadata->{ct} || '' };
}

sub read {
    my ($class, $input, $done) = @_;
    my $id = $input->{id};
    die 'invalid metadata identity' unless defined($id) && !ref($id) && length($id) <= 16384
        && $id =~ /^[a-zA-Z][a-zA-Z0-9+.-]*:/;
    my $handler = Slim::Player::ProtocolHandlers->handlerForURL($id);
    return $done->({available => JSON::PP::false}) unless $handler && $handler->can('getMetadataFor');
    my $metadata = $handler->getMetadataFor(Chilpa::Client->current, $id);
    return $done->({available => JSON::PP::false}) unless ref($metadata) eq 'HASH';
    # Whitelist through the same presentation contract. Raw metadata may carry
    # account data, signed URLs or nested implementation objects.
    my %item = %$metadata;
    $item{content_type} ||= $metadata->{ct};
    my $presentation = Chilpa::Presentation::describe(\%item, 1, 0);
    $presentation->{stable_id} = $id;
    $done->({available => JSON::PP::true, presentation => $presentation});
}

1;
