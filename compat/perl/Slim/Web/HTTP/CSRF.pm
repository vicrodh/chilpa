package Slim::Web::HTTP::CSRF;

use strict;
use warnings;

# These are route identifiers, not HTTP endpoints. The host accepts actions
# only against the currently rendered form revision over its private channel.
sub protectName { $_[1] }
sub protectURI { $_[1] }

1;
