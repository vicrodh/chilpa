package Chilpa::Cookies;

use strict;
use warnings;
use parent 'HTTP::Cookies';

sub new {
    my ($class, %args) = @_;
    my $self = bless HTTP::Cookies->new, $class;
    @$self{qw(load_private save_private)} = @args{qw(load save)};
    return $self;
}

sub restore {
    my ($self) = @_;
    return if $self->{restored};
    my $rows = $self->{load_private}->() // [];
    die 'invalid cookie storage' unless ref($rows) eq 'ARRAY' && @$rows <= 256;
    for my $row (@$rows) {
        die 'invalid stored cookie' unless ref($row) eq 'ARRAY' && @$row == 11;
        my @cookie = @$row;
        next if $cookie[9];
        if (defined $cookie[8]) {
            next if $cookie[8] <= time;
            $cookie[8] -= time;
        }
        $self->SUPER::set_cookie(@cookie);
    }
    $self->{restored} = 1;
}

sub persist {
    my ($self) = @_;
    my @rows;
    $self->SUPER::scan(sub {
        return if $_[9] || (defined($_[8]) && $_[8] <= time);
        push @rows, [@_];
    });
    die 'cookie storage limit exceeded' if @rows > 256;
    die 'cookie storage failed' unless $self->{save_private}->(\@rows);
}

sub set_cookie {
    my $self = shift;
    $self->restore;
    $self->SUPER::set_cookie(@_);
    $self->persist;
    return $self;
}

sub clear {
    my $self = shift;
    $self->restore;
    $self->SUPER::clear(@_);
    $self->persist;
    return $self;
}

sub add_cookie_header {
    my $self = shift;
    $self->restore;
    return $self->SUPER::add_cookie_header(@_);
}

# This adapter never writes the superclass's plaintext cookie file.
sub save { $_[0]->persist }
sub load { $_[0]->restore }

1;
