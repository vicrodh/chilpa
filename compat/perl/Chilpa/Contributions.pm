package Chilpa::Contributions;

use strict;
use warnings;
use JSON::PP ();

our $ACTIVE;

sub new { bless { entries => {} }, $_[0] }

sub register {
    my ($self, $kind, $key, $definition) = @_;
    die 'invalid contribution key' unless defined($key) && length($key) && length($key) <= 256;
    my $entries = $self->{entries}{$kind} ||= {};
    die 'too many plugin contributions' if !exists($entries->{$key}) && keys(%$entries) >= 128;
    $entries->{$key} = $definition;
    return 1;
}

sub entries { $_[0]{entries}{$_[1]} || {} }

# These are observed registrations, not a claim that an operation has passed.
# The host still approves access and records compatibility evidence separately.
sub features {
    my ($self) = @_;
    my $features = { map {
        $_ => keys(%{ $self->entries($_) }) ? JSON::PP::true : JSON::PP::false
    } qw(browse search resolve metadata) };
    $features->{resolve} = JSON::PP::true if keys %{ $self->entries('url') };
    return $features;
}

1;
