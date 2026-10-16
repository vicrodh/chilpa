package Slim::Utils::Accessor;

use strict;
use warnings;

sub new { bless {}, ref($_[0]) || $_[0] }

sub mk_accessor {
    my ($class, $mode, @names) = @_;
    die 'unsupported accessor mode' unless $mode eq 'rw' || $mode eq 'hash';
    for my $name (@names) {
        die 'invalid accessor name' unless $name =~ /^[a-zA-Z_]\w*$/;
        no strict 'refs';
        *{"${class}::$name"} = sub {
            my ($self, @value) = @_;
            if ($mode eq 'hash') {
                $self->{$name} ||= {};
                return $self->{$name} unless @value;
                my $key = shift @value;
                $self->{$name}{$key} = $value[0] if @value;
                return $self->{$name}{$key};
            }
            $self->{$name} = $value[0] if @value;
            return $self->{$name};
        };
    }
}

1;
