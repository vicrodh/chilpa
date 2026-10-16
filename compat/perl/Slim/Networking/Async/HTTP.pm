package Slim::Networking::Async::HTTP;

use strict;
use warnings;
use HTTP::Cookies;
use HTTP::Response;
use URI;
use Slim::Networking::SimpleAsyncHTTP;

our $COOKIES = HTTP::Cookies->new;

sub new { bless {}, $_[0] }
sub hasSSL { defined $Slim::Networking::SimpleAsyncHTTP::TRANSPORT ? 1 : 0 }
sub cookie_jar { $COOKIES }
sub response { $_[0]{response} }

sub send_request {
    my ($self, $args) = @_;
    my $request = $args->{request}->clone;
    my $redirects = 0;
    my $send;
    $send = sub {
        my $http = Slim::Networking::SimpleAsyncHTTP->new(sub {
            my ($reply) = @_;
            $self->{response} = HTTP::Response->new($reply->code, undef, $reply->headers, $reply->content);
            $self->{response}->request($request);
            if ($self->{response}->is_redirect && (my $location = $self->{response}->header('Location'))) {
                $args->{onRedirect}->($self) if $args->{onRedirect};
                if (++$redirects > 5) {
                    $args->{onError}->($self, 'HTTP redirect limit exceeded') if $args->{onError};
                    undef $send;
                    return;
                }
                my $target = URI->new_abs($location, $request->uri);
                if ($target->scheme !~ /^https?$/ || defined($target->userinfo)) {
                    $args->{onError}->($self, 'invalid HTTP redirect') if $args->{onError};
                    undef $send;
                    return;
                }
                if ($target->scheme ne $request->uri->scheme || $target->authority ne $request->uri->authority) {
                    $request->remove_header('Authorization', 'Cookie', 'Proxy-Authorization');
                }
                if ($reply->code == 303 && $request->method ne 'HEAD') {
                    $request->method('GET');
                    $request->content('');
                    $request->remove_header('Content-Type', 'Content-Length');
                }
                $request->uri($target);
                return $send->();
            }
            $args->{onBody}->($self) if $args->{onBody};
            undef $send;
        }, sub {
            my ($reply, $error) = @_;
            $args->{onError}->($self, $error) if $args->{onError};
            undef $send;
        }, { timeout => $args->{timeout} // 30, follow_redirects => 0 });
        my @headers = map { $_ => scalar $request->header($_) } $request->header_field_names;
        $http->_request($request->method, "" . $request->uri, $request->content, @headers);
    };
    $send->();
}

1;
