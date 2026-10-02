use strict;
use warnings;
use Test::More;
use Slim::Networking::SimpleAsyncHTTP;

my ($request, $complete, $success, $failure, $error);
local $Slim::Networking::SimpleAsyncHTTP::TRANSPORT = sub { ($request, $complete) = @_ };
my $http = Slim::Networking::SimpleAsyncHTTP->new(
    sub { $success++ }, sub { $failure++; $error = $_[1] }, { timeout => 15, context => 'poll' },
);
$http->post('https://example.org/token', Authorization => 'private authorization', 'grant_type=device');
is($request->{body}, 'grant_type=device', 'form body is forwarded unchanged');
is($request->{headers}{Authorization}, 'private authorization', 'plugin authentication reaches the host transport');
is($request->{timeout}, 15, 'guest timeout is supplied to host policy');
ok(!$success, 'HTTP completion is asynchronous');
eval { $http->get('https://example.org/other') };
like($@, qr/already pending/, 'overlapping use cannot replace pending state');
$complete->({ status => 400, body => '{"error":"pending"}', headers => { 'Content-Type' => 'application/json' } });
is($failure, 1, 'HTTP errors invoke the failure callback');
is($http->content, '{"error":"pending"}', 'error response body remains available to the guest');
is($http->headers->content_type, 'application/json', 'response headers remain available');
is($error, 'HTTP 400', 'error string contains no request secrets');
$complete->({ status => 200 });
ok(!$success, 'duplicate completion is ignored');
$http->get('https://example.org/status');
$complete->({ status => 200, body => 'ready' });
is($success, 1, 'a completed object can issue another request');
is($http->params('context'), 'poll', 'guest callback context survives');
$http->post('https://example.org/authorize', Authorization => 'private', Cookie => 'session=private', 'password=private');
$complete->({status => 303, headers => {Location => 'https://other.example.org/complete'}});
is($request->{method}, 'GET', 'a 303 changes the next hop to GET');
is($request->{body}, '', 'the authentication body is not resent after a 303');
ok(!exists $request->{headers}{Authorization} && !exists $request->{headers}{Cookie}, 'cross-origin redirects discard explicit credentials');
is($request->{url}, 'https://other.example.org/complete', 'every redirect returns through the host transport');
$complete->({status => 200, body => 'done'});
is($success, 2, 'only the final response completes the request');
$http->get('https://example.org/loop');
for (1..6) { $complete->({status => 302, headers => {Location => '/loop'}}) }
is($error, 'HTTP redirect limit exceeded', 'a redirect cycle has a finite limit');
done_testing;
