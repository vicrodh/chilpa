package Slim::Utils::Cache;

use strict;
use warnings;
use Time::HiRes qw(time);
use Storable qw(nfreeze thaw);

our %CACHES;
our ($BYTES, $COUNT, $CLOCK) = (0, 0, 0);
use constant MAX_BYTES => 16 * 1024 * 1024;
use constant MAX_ENTRIES => 4096;

sub new {
    my ($class, $namespace) = @_;
    return $CACHES{$namespace // 'default'} ||= bless {}, $class;
}

sub get {
    my ($self, $key) = @_;
    my $entry = $self->{$key} or return;
    if ($entry->{expires} && $entry->{expires} <= time) {
        $self->remove($key);
        return;
    }
    $entry->{used} = ++$CLOCK;
    return eval { ${thaw($entry->{encoded})} };
}

# Expiries as the host's cache writes them: "90 minutes", "1 day", "3M"
# (months; a lowercase m is minutes), "1y".
my %UNIT = (s => 1, sec => 1, second => 1, m => 60, min => 60, minute => 60, h => 3600, hour => 3600,
    d => 86400, day => 86400, w => 604800, week => 604800, M => 2592000, month => 2592000, y => 31536000, year => 31536000);
sub seconds {
    my ($ttl) = @_;
    my ($count, $unit) = $ttl =~ /^\s*(\d+(?:\.\d+)?)\s*([A-Za-z]+)\s*$/ or return;
    $unit =~ s/s$// if length($unit) > 2;
    my $each = $UNIT{$unit} // $UNIT{lc $unit} or return;
    return $count * $each;
}

sub set {
    my ($self, $key, $value, $ttl) = @_;
    $ttl = 3600 unless defined $ttl;
    my $expiry;
    if (lc($ttl) eq 'never' || $ttl eq '-1') { $expiry = 0 }
    elsif (lc($ttl) eq 'now') { $expiry = time }
    elsif ($ttl =~ /^\d+(?:\.\d+)?$/) { $expiry = $ttl > 30 * 86400 ? $ttl : time + $ttl }
    elsif (my $seconds = seconds($ttl)) { $expiry = time + $seconds }
    # Caching is optional: an expiry this does not read keeps the default.
    else { $expiry = time + 3600 }
    $self->remove($key);
    # Caching is optional. An uncacheable or oversized value must not abort
    # a successful catalog response. Serialization stays inside the worker.
    my $encoded = eval { nfreeze(\$value) };
    return 1 unless defined $encoded;
    my $size = length($encoded) + length($key) + 128;
    return 1 if $size > MAX_BYTES;
    if ($COUNT >= MAX_ENTRIES || $BYTES + $size > MAX_BYTES) {
        my @oldest = sort { $a->[2] <=> $b->[2] } map {
            my $cache = $_;
            map { [$cache, $_, $cache->{$_}{used}] } keys %$cache
        } values %CACHES;
        for my $entry (@oldest) {
            last if $COUNT < MAX_ENTRIES && $BYTES + $size <= MAX_BYTES;
            $entry->[0]->remove($entry->[1]);
        }
    }
    $self->{$key} = { encoded => $encoded, expires => $expiry, size => $size, used => ++$CLOCK };
    $BYTES += $size;
    $COUNT++;
    return 1;
}

sub remove {
    my $entry = delete $_[0]{$_[1]} or return;
    $BYTES -= $entry->{size};
    $COUNT--;
    return eval { ${thaw($entry->{encoded})} };
}
sub clear { my ($self) = @_; $self->remove($_) for keys %$self }
sub purge {
    my ($self) = @_;
    my @expired = grep { $self->{$_}{expires} && $self->{$_}{expires} <= time } keys %$self;
    $self->remove($_) for @expired;
    return scalar @expired;
}

1;
