package Slim::Networking::SimpleAsyncHTTP;

use strict;
use warnings;
use HTTP::Headers;
use HTTP::Request;
use HTTP::Response;
use URI;

# The worker supplies a transport over the host's private channel. This module
# cannot open a socket; permissions and response limits belong to the host
# transport. Every redirect is checked there again as a separate request.
our $TRANSPORT;

sub new {
    my ($class, $success, $failure, $options) = @_;
    return bless { success => $success, failure => $failure, options => $options || {} }, $class;
}

sub get { my ($self, $url, @headers) = @_; $self->_request('GET', $url, undef, @headers) }
sub head { my ($self, $url, @headers) = @_; $self->_request('HEAD', $url, undef, @headers) }
sub post {
    my ($self, $url, @args) = @_;
    my $body = @args % 2 ? pop @args : '';
    $self->_request('POST', $url, $body, @args);
}

sub _request {
    my ($self, $method, $url, $body, @headers) = @_;
    die 'HTTP transport is unavailable' unless $TRANSPORT;
    die 'HTTP request is already pending' if $self->{pending};
    die 'invalid HTTP headers' if @headers % 2;
    $self->{pending} = 1;
    my $message = HTTP::Request->new($method, $url, \@headers, $body);
    $self->_send($message, 5);
    return $self;
}

sub _send {
    my ($self, $message, $remaining) = @_;
    $self->{url} = '' . $message->uri;
    my $cookies = $Slim::Networking::Async::HTTP::COOKIES;
    $cookies->add_cookie_header($message) if $cookies;
    my $request = {
        method => $message->method, url => '' . $message->uri,
        headers => { map { $_ => scalar $message->header($_) } $message->header_field_names }, body => $message->content,
        timeout => $self->{options}{timeout} // 30,
        follow_redirects => 0,
    };
    my $completed = 0;
    $TRANSPORT->($request, sub {
        my ($reply) = @_;
        return if $completed++;
        $self->{code} = $reply->{status} || 0;
        $self->{content} = $reply->{body} // '';
        $self->{headers} = HTTP::Headers->new(%{ $reply->{headers} || {} });
        if ($cookies) {
            my $response = HTTP::Response->new($self->{code}, undef, $self->{headers});
            $response->request($message);
            $cookies->extract_cookies($response);
        }
        if (!$reply->{error} && ($self->{options}{follow_redirects} // 1)
            && $self->{code} =~ /^(301|302|303|307|308)$/ && (my $location = $self->{headers}->header('Location'))) {
            return $self->_fail('HTTP redirect limit exceeded') unless $remaining;
            my $target = URI->new_abs($location, $message->uri);
            return $self->_fail('invalid HTTP redirect') unless $target->scheme =~ /^https?$/ && !defined($target->userinfo);
            my $next = $message->clone;
            if (lc($target->scheme) ne lc($message->uri->scheme) || lc($target->authority) ne lc($message->uri->authority)) {
                $next->remove_header('Authorization', 'Cookie', 'Proxy-Authorization');
            }
            if ($self->{code} == 303 && $next->method ne 'HEAD') {
                $next->method('GET');
                $next->content('');
                $next->remove_header('Content-Type', 'Content-Length');
            }
            $next->uri($target);
            return $self->_send($next, $remaining - 1);
        }
        if ($reply->{error} || !$self->{code} || $self->{code} >= 400) {
            return $self->_fail($self->{code} ? "HTTP $self->{code}" : 'HTTP transport failed');
        }
        delete $self->{pending};
        delete $self->{error};
        return $self->{success}->($self);
    });
}

sub _fail {
    my ($self, $error) = @_;
    delete $self->{pending};
    $self->{error} = $error;
    return $self->{failure}->($self, $error);
}

sub content { $_[0]{content} }
sub contentRef { \$_[0]{content} }
sub code { $_[0]{code} }
sub url { $_[0]{url} }
sub error { $_[0]{error} }
sub headers { $_[0]{headers} }
sub params { defined($_[1]) ? $_[0]{options}{$_[1]} : $_[0]{options} }

1;
