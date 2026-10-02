package Slim::Web::Pages;

use strict;
use warnings;
use Chilpa::Web;

sub addPageFunction { shift; $Chilpa::Web::ACTIVE->register(@_, 0) }
sub addRawFunction { shift; $Chilpa::Web::ACTIVE->register(@_, 1) }

1;
