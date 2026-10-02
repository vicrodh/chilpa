package Slim::Menu::Base;

use strict;
use warnings;
use Chilpa::Contributions;

sub registerInfoProvider {
    my ($class, $key, %definition) = @_;
    die 'invalid menu callback' unless ref($definition{func}) eq 'CODE';
    my $kind = $class eq 'Slim::Menu::GlobalSearch' ? 'search' : 'menu';
    return $Chilpa::Contributions::ACTIVE->register($kind, "$class/$key", \%definition);
}

1;
