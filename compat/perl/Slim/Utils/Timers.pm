package Slim::Utils::Timers;

use strict;
use warnings;
use Scalar::Util qw(refaddr);

our @PENDING;
our $NEXT_ID = 0;

sub identity { ref($_[0]) ? 'r:' . refaddr($_[0]) : 's:' . ($_[0] // '') }

sub setTimer {
    my ($owner, $at, $callback, @args) = @_;
    die 'invalid timer callback' unless ref($callback) eq 'CODE';
    die 'invalid timer deadline' unless defined($at) && $at =~ /^\d+(?:\.\d+)?$/;
    die 'too many pending timers' if @PENDING >= 1024;
    my $timer = { id => ++$NEXT_ID, owner => $owner, at => $at, callback => $callback, args => \@args };
    push @PENDING, $timer;
    return $timer;
}

sub killTimers {
    my ($owner, $callback) = @_;
    @PENDING = grep {
        identity($_->{owner}) ne identity($owner)
            || (defined($callback) && identity($_->{callback}) ne identity($callback))
    } @PENDING;
}

sub killSpecific {
    my ($timer) = @_;
    @PENDING = grep { $_ != $timer } @PENDING;
}

sub next_deadline {
    return unless @PENDING;
    return (sort { $a <=> $b } map { $_->{at} } @PENDING)[0];
}

# I take a snapshot of IDs, so a callback may cancel another ready timer or
# schedule itself without making a single event-loop turn run indefinitely.
sub fire_due {
    my ($now) = @_;
    my @ready = map { $_->{id} }
        sort { $a->{at} <=> $b->{at} || $a->{id} <=> $b->{id} }
        grep { $_->{at} <= $now } @PENDING;
    for my $id (@ready) {
        my ($timer) = grep { $_->{id} == $id } @PENDING;
        next unless $timer;
        killSpecific($timer);
        $timer->{callback}->($timer->{owner}, @{ $timer->{args} });
    }
}

1;
