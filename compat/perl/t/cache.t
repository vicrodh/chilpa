use strict;
use warnings;
use Test::More;
use Slim::Utils::Cache;

my $cache = Slim::Utils::Cache->new('guest');
$cache->set('credential', 'value', 3600);
is($cache->get('credential'), 'value', 'credential cache survives a subsequent request');
$cache->set('expired', 'value', time - 10);
ok(!defined $cache->get('expired'), 'absolute expiry does not become a multi-year relative lifetime');
$cache->set('recent', [], 'never');
is_deeply($cache->get('recent'), [], 'unexpired history remains available');
$cache->set('immediate', 'value', 0);
ok(!defined $cache->get('immediate'), 'zero lifetime expires immediately');
$cache->clear;
ok(!defined $cache->get('credential'), 'account changes can clear cached credentials');
$cache->set("track:$_", {title=>"Recording $_"}, 3600) for 1..4096;
$cache->get('track:1');
my $ok = eval { $cache->set('track:4097', {title=>'New recording'}, 3600); 1 };
ok($ok, 'a large library evicts cache data instead of terminating a callback');
ok(defined $cache->get('track:1'), 'recently used metadata survives eviction');
ok(!defined $cache->get('track:2'), 'least recently used metadata is evicted');
is($Slim::Utils::Cache::COUNT, 4096, 'the global entry bound remains enforced');
$cache->set('large', 'x' x (17 * 1024 * 1024), 3600);
ok(!defined $cache->get('large'), 'oversized catalog responses remain uncached');
ok($Slim::Utils::Cache::BYTES <= Slim::Utils::Cache::MAX_BYTES, 'the serialized cache budget is bounded');
$cache->clear;
my $album = { genre => { name => 'Rock' }, images => ['cover'] };
$cache->set('album', $album, 300);
$album->{images}[0] = 'changed after insertion';
my $first = $cache->get('album');
$first->{genre} = $first->{genre}{name};
$first->{images}[0] = 'changed after retrieval';
is_deeply($cache->get('album'), { genre => { name => 'Rock' }, images => ['cover'] },
    'normalizing a response never mutates the cached response');
is_deeply($cache->remove('album'), { genre => { name => 'Rock' }, images => ['cover'] },
    'removal returns the stored snapshot');
is($Slim::Utils::Cache::BYTES, 0, 'clear releases the accounted cache bytes');
done_testing;
