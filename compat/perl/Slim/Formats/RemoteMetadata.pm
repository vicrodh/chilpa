package Slim::Formats::RemoteMetadata;

use strict;
use warnings;
use Chilpa::Contributions;

sub registerProvider {
    my ($class, %definition) = @_;
    die 'invalid metadata callback' unless ref($definition{func}) eq 'CODE';
    my $entries = $Chilpa::Contributions::ACTIVE->entries('metadata');
    return $Chilpa::Contributions::ACTIVE->register('metadata', 'provider-' . scalar(keys %$entries), \%definition);
}

1;
