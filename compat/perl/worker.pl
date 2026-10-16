use strict;
use warnings;
use FindBin;
use lib $FindBin::Bin;
use File::Spec;
use Chilpa::Channel;
use Chilpa::Worker;

binmode STDIN;
binmode STDOUT;
open my $output, '>&', \*STDOUT or die 'worker output unavailable';
open STDOUT, '>', File::Spec->devnull or die 'worker output could not be isolated';
open STDERR, '>', File::Spec->devnull or die 'worker diagnostics could not be isolated';
my $channel = Chilpa::Channel->new(read => \*STDIN, write => $output);
Chilpa::Worker->new($channel, "$FindBin::Bin/templates")->run;
