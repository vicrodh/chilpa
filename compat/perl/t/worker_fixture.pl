use strict;
use warnings;
use lib shift @ARGV;
use Chilpa::Channel;
use Chilpa::Source;
use Chilpa::Contributions;
$Chilpa::Contributions::ACTIVE = Chilpa::Contributions->new;
use Slim::Utils::Prefs;

binmode STDIN;
binmode STDOUT;
my $channel = Chilpa::Channel->new(read=>\*STDIN, write=>\*STDOUT);
$Slim::Utils::Prefs::LOAD = sub { $channel->service_sync({name=>'private_get',key=>$_[0]}) };
$Slim::Utils::Prefs::SAVE = sub { $channel->service_sync({name=>'private_set',key=>$_[0],value=>$_[1]}) };
while (1) {
    my $call = $channel->receive;
    die 'unexpected message' unless $call->{type} eq 'call';
    my $result;
    if ($call->{operation} eq 'initialize') {
        preferences('server')->init({language => 'EN'});
        $result = {features => {browse => JSON::PP::true}, settings_pages => ['settings.html'], limitations => [], diagnostics => {}};
    }
    elsif ($call->{operation} eq 'source.browse') {
        $result = Chilpa::Source->new->listing({title => 'Library', items => [
            {url => 'guest://recording:17', name => 'Recording', type => 'audio'},
            {name => 7, label => 'TRACKS_COUNT', type => 'text'},
        ]}, {at => 'album'});
    }
    elsif ($call->{operation} eq 'source.metadata') {
        $result = {available => JSON::PP::true, private => 'discard', presentation => {
            entity => 'track', advertised_quality => {sample_rate_hz => 88200, bit_depth => 24},
            private => 'discard', actions => [{key => 'write', enabled => JSON::PP::true}]}};
    }
    elsif ($call->{operation} eq 'set') {
        preferences($call->{input}{namespace})->set($call->{input}{key}, $call->{input}{value});
        $result = {saved=>JSON::PP::true};
    }
    elsif ($call->{operation} eq 'get') {
        $result = preferences($call->{input}{namespace})->get($call->{input}{key});
    }
    elsif ($call->{operation} eq 'wrong_id') {
        $call->{id}++;
        $result = {};
    }
    else { die 'unknown fixture operation' }
    $channel->send({type=>'reply',id=>$call->{id},result=>$result});
}
