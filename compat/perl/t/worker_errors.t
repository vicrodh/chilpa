use strict;
use warnings;
use Test::More;
use Chilpa::Worker;

{
    package FailureChannel;
    sub dispatch_reply { die "private credential must never leave this process\n" }
    sub send { push @{$_[0]{replies}}, $_[1] }
}
my $channel = bless {replies => []}, 'FailureChannel';
my $worker = bless {channel => $channel, pending => 7}, 'Chilpa::Worker';
$worker->{done} = sub {
    delete $worker->{pending};
    delete $worker->{done};
    $channel->send({type => 'reply', id => 7, result => $_[0]});
};
my $ok = eval { $worker->dispatch({type => 'service_reply'}); 1 };
ok($ok, 'an asynchronous guest exception does not terminate the worker');
is($channel->{replies}[0]{result}{error}, 'Compatibility worker callback failed: plugin exception.', 'pending operation receives a sanitized failure');
ok(!$worker->{pending}, 'failed operation releases its pending slot');
$worker->dispatch({type => 'call', id => 8, operation => 'unsupported', input => {}});
is($channel->{replies}[1]{id}, 8, 'the following operation receives its own reply');
$worker->failed('timer', 'secret timer payload');
is_deeply($worker->{last_failure}, {phase=>'timer',kind=>'plugin exception'}, 'idle timer failure retains only a fixed diagnostic');
{
    package DelayedSource;
    sub browse { push @{$_[0]{callbacks}}, $_[2] }
}
$worker->{web} = {};
$worker->{source} = bless {callbacks=>[]}, 'DelayedSource';
$worker->dispatch({type=>'call',id=>9,operation=>'source.browse',input=>{}});
my $old = $worker->{source}{callbacks}[0];
$old->({entries=>[]});
$worker->dispatch({type=>'call',id=>10,operation=>'source.browse',input=>{}});
$old->({entries=>[]});
is($worker->{pending}, 10, 'a repeated late callback cannot consume the following operation');
$worker->{source}{callbacks}[1]->({entries=>[]});
is($channel->{replies}[-1]{id}, 10, 'the current callback retains its own reply identity');
is(scalar @{$channel->{replies}}, 4, 'a repeated callback does not emit an extra reply');
done_testing;
