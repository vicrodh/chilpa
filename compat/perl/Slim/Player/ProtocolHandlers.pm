package Slim::Player::ProtocolHandlers;

use strict;
use warnings;
use Chilpa::Contributions;

sub registerHandler {
    my ($class, $scheme, $handler) = @_;
    die 'invalid media scheme' unless $scheme =~ /^[a-zA-Z][a-zA-Z0-9+.-]*$/;
    return $Chilpa::Contributions::ACTIVE->register('resolve', lc($scheme), $handler);
}

sub handlerForURL {
    my ($class, $url) = @_;
    my $registered = $Chilpa::Contributions::ACTIVE->entries('url');
    for my $key (sort keys %$registered) {
        my $entry = $registered->{$key};
        return $entry->{handler} if $url =~ $entry->{match};
    }
    my ($scheme) = $url =~ /^([a-zA-Z][a-zA-Z0-9+.-]*):/;
    return unless defined $scheme;
    return $Chilpa::Contributions::ACTIVE->entries('resolve')->{lc $scheme};
}

sub registerURLHandler {
    my ($class, $match, $handler) = @_;
    die 'invalid media URL pattern' unless ref($match) eq 'Regexp';
    my $entries = $Chilpa::Contributions::ACTIVE->entries('url');
    my $key = sprintf('provider-%03d', scalar keys %$entries);
    return $Chilpa::Contributions::ACTIVE->register('url', $key, { match => $match, handler => $handler });
}

sub registerIconHandler {
    my ($class, $match, $callback) = @_;
    my $entries = $Chilpa::Contributions::ACTIVE->entries('icon');
    return $Chilpa::Contributions::ACTIVE->register('icon', 'provider-' . scalar(keys %$entries), { match => $match, callback => $callback });
}

1;
