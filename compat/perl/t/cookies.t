use strict;
use warnings;
use Test::More;
use JSON::PP;
use HTTP::Request;
use Chilpa::Cookies;
use Slim::Networking::Async::HTTP;

my ($stored, $reads);
my $new = sub { Chilpa::Cookies->new(
    load => sub { $reads++; $stored },
    save => sub { $stored = decode_json(encode_json($_[0])); 1 },
) };
my $jar = $new->();
ok(!$reads, 'construction does not issue services outside a host call');
$jar->set_cookie(0, 'identity', 'private token', '/', 'example.org');
is(scalar @$stored, 1, 'manually entered identity cookies use the private store');
my $restarted = $new->();
my $request = HTTP::Request->new(GET => 'https://example.org/music');
$restarted->add_cookie_header($request);
like($request->header('Cookie'), qr/private token/, 'account cookies survive a worker restart');
$request = HTTP::Request->new(GET => 'https://other.example.net/music');
$restarted->add_cookie_header($request);
ok(!$request->header('Cookie'), 'restoring storage preserves domain scope');
$restarted->clear('example.org', '/', 'identity');
is(scalar @$stored, 0, 'logout removes the persisted cookie');
$jar = $new->();
$request = HTTP::Request->new(GET => 'https://example.org/music');
$jar->add_cookie_header($request);
ok(!$request->header('Cookie'), 'logout remains effective after restart');
local $Slim::Networking::Async::HTTP::COOKIES = $jar;
local $Slim::Networking::SimpleAsyncHTTP::TRANSPORT = sub {
    $_[1]->({status => 200, headers => {'Set-Cookie' => ['account=one; Path=/; Max-Age=3600', 'session=two; Path=/; Max-Age=3600']}});
};
Slim::Networking::SimpleAsyncHTTP->new(sub {}, sub { die 'HTTP fixture failed' })->get('https://example.org/login');
is(scalar @$stored, 2, 'multiple response cookies survive the channel and private persistence');
done_testing;
