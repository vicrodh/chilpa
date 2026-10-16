package Slim::Plugin::OPMLBased;

use strict;
use warnings;
use parent 'Slim::Plugin::Base';
use Chilpa::Contributions;

sub initPlugin {
    my ($class, %definition) = @_;
    die 'unsupported source feed' unless ref($definition{feed}) eq 'CODE';
    return $Chilpa::Contributions::ACTIVE->register('browse', $class, \%definition);
}

sub feed { $Chilpa::Contributions::ACTIVE->entries('browse')->{$_[0]}{feed} }

1;
