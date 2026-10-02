package Slim::Web::HTTP;

use strict;
use warnings;
use Chilpa::Web;

sub filltemplatefile { $Chilpa::Web::ACTIVE->render(@_) }

1;
