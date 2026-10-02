package Chilpa::Web;

use strict;
use warnings;
use HTTP::Response;
use URI;
use Chilpa::Settings;

# One worker hosts one plugin. Registered pages never become listening sockets.
our $ACTIVE;

sub new {
    my ($class, %args) = @_;
    return bless { %args, pages => {}, raw => {}, revision => 0 }, $class;
}

sub register {
    my ($self, $page, $handler, $raw) = @_;
    die 'invalid settings page' if !$page || $page =~ m{(?:^/|\\|(?:^|/)\.\.(?:/|$)|[?#])};
    my $routes = $raw ? $self->{raw} : $self->{pages};
    die 'duplicate settings page' if exists $routes->{$page};
    $routes->{$page} = $handler;
}

sub render {
    my ($self, $page, $params) = @_;
    return Chilpa::Settings::render($self->{root}, $self->{shared}, $page, $params, $self->{strings});
}

sub document {
    my ($self, $page) = @_;
    $page =~ s{^/}{};
    die 'invalid help document' unless $page =~ /\.html$/;
    return $self->render($page, {})->{public};
}

# Completion may occur later through the guest's HTTP callback. The worker's
# event loop owns the deadline and kills requests which do not complete.
sub open {
    my ($self, $page, $params, $done, $hops) = @_;
    $hops ||= 0;
    die 'settings redirect limit exceeded' if $hops > 8;
    die 'settings request is already pending' if $self->{pending};
    my $handler = $self->{pages}{$page} or die 'unknown settings page';
    $self->{pending} = 1;
    delete $self->{current};
    my $response = HTTP::Response->new(200);
    my $completed = 0;
    my $finish = sub {
        my ($body) = @_;
        return if $completed++;
        delete $self->{pending};
        if ($response->is_redirect) {
            my $location = $response->header('Location') // '';
            my $base = URI->new("https://settings.invalid/$page");
            my $target = URI->new_abs($location, $base);
            die 'external settings redirect' unless $target->scheme eq 'https'
                && $target->authority eq $base->authority && !$target->query && !$target->fragment;
            my $next = $target->path;
            $next =~ s{^/}{};
            return $self->open($next, {}, $done, $hops + 1);
        }
        die 'settings handler did not render a form' unless ref($body) eq 'HASH' && $body->{public};
        $self->{current} = { page => $page, rendered => $body };
        $body->{public}{revision} = ++$self->{revision};
        $done->($body->{public});
    };
    local $ACTIVE = $self;
    my $callback = sub { local $ACTIVE = $self; $finish->($_[2]) };
    my $body = $handler->handler(undef, $params, $callback, undef, $response);
    $finish->($body) if defined $body;
}

sub submit {
    my ($self, $revision, $action, $values, $done) = @_;
    die 'stale settings form' unless $self->{current} && $revision == $self->{revision};
    my $current = $self->{current};
    my $params = Chilpa::Settings::submission($current->{rendered}, $action, $values);
    delete $self->{current};
    $self->open($current->{page}, $params, $done);
}

1;
