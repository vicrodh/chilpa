use strict;
use warnings;
use Test::More;
use HTTP::Request;
use Slim::Networking::Async::HTTP;

my @pending;
local $Slim::Networking::SimpleAsyncHTTP::TRANSPORT = sub { push @pending, [@_] };
Slim::Networking::Async::HTTP->cookie_jar->set_cookie(0, 'identity', 'private cookie', '/', 'example.org');
my ($redirect, $finished);
my $http = Slim::Networking::Async::HTTP->new;
$http->send_request({
    request => HTTP::Request->new(HEAD => 'https://example.org/stream', [Authorization => 'private authorization']),
    onRedirect => sub { $redirect = $http->response->header('Location') },
    onBody => sub { $finished++ },
    onError => sub { fail('request unexpectedly failed') },
});
my ($request, $complete) = @{shift @pending};
is($request->{follow_redirects}, 0, 'host returns each redirect for separate permission checks');
like($request->{headers}{Cookie}, qr/private cookie/, 'plugin cookie jar supplies the account cookie');
$complete->({status=>302,headers=>{Location=>'https://media.example.net/audio'},body=>''});
is($redirect, 'https://media.example.net/audio', 'original callback can observe the stream location');
($request, $complete) = @{shift @pending};
is($request->{url}, $redirect, 'next hop returns to the permission-checking host transport');
ok(!exists $request->{headers}{Authorization}, 'authorization is removed when the origin changes');
ok(!exists $request->{headers}{Cookie}, 'account cookies do not follow an unrelated origin');
is($request->{method}, 'HEAD', 'media resolution does not download the audio body');
$complete->({status=>200,headers=>{},body=>''});
is($finished, 1, 'final response completes once');
done_testing;
