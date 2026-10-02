use strict;
use warnings;
use Test::More;
use JSON::PP;
use Chilpa::Channel;

sub frame { my $bytes = encode_json($_[0]); pack('N', length($bytes)) . $bytes }
my $incoming = frame({type=>'service_reply',id=>1,result=>{ok=>JSON::PP::true,value=>{account=>'stored'}}});
open my $reader, '<', \$incoming or die $!;
my $outgoing = '';
open my $writer, '>', \$outgoing or die $!;
my $channel = Chilpa::Channel->new(read=>$reader,write=>$writer);
is_deeply($channel->service_sync({name=>'private_get',key=>'preferences'}), {account=>'stored'}, 'private value arrives only through the host channel');
my $sent = decode_json(substr($outgoing,4));
is($sent->{operation}{key}, 'preferences', 'service request retains its key');
ok(!exists $sent->{operation}{owner}, 'worker cannot select an owner');
eval { $channel->dispatch_reply({type=>'service_reply',id=>1,result=>{ok=>JSON::PP::true}}) };
like($@, qr/unknown host service reply/, 'a service completion cannot be replayed');
my $oversized = pack('N', Chilpa::Channel::MAX_FRAME + 1);
open my $large, '<', \$oversized or die $!;
my $bad = Chilpa::Channel->new(read=>$large,write=>$writer);
eval { $bad->receive };
like($@, qr/invalid worker message length/, 'oversized frame is refused before reading its body');
my $idle = frame({type=>'call',id=>7,operation=>'tick',input=>{}})
    . frame({type=>'service_reply',id=>1,result=>{ok=>JSON::PP::true,value=>JSON::PP::true}});
open my $idle_reader, '<', \$idle or die $!;
my $timer = Chilpa::Channel->new(read=>$idle_reader,write=>$writer);
ok($timer->service_sync({name=>'private_set',key=>'preferences',value=>{}}), 'timer storage completes even when a new host call arrives first');
ok($timer->readable(0), 'deferred calls are visible to the event loop without socket readiness');
is($timer->receive->{id}, 7, 'deferred call runs after the storage callback, without reentry');
done_testing;
