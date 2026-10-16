use strict;
use warnings;
use Test::More;
use Chilpa::Contributions;
use Slim::Plugin::OPMLBased;
use Slim::Menu::GlobalSearch;
use Slim::Menu::TrackInfo;
use Slim::Player::ProtocolHandlers;

local $Chilpa::Contributions::ACTIVE = Chilpa::Contributions->new;
my $registry = $Chilpa::Contributions::ACTIVE;
ok(!$registry->features->{browse}, 'empty plugin contributes no source');
Slim::Menu::TrackInfo->registerInfoProvider('information', func => sub {});
ok(!$registry->features->{search}, 'context menu is not mistaken for global search');
{
    package Guest::Source;
    our @ISA = ('Slim::Plugin::OPMLBased');
}
my $feed = sub {};
Guest::Source->initPlugin(feed => $feed, tag => 'guest');
Slim::Menu::GlobalSearch->registerInfoProvider('search', func => sub {});
ok($registry->features->{browse} && $registry->features->{search}, 'source roles come from registered interfaces');
is(Guest::Source->feed, $feed, 'registered feed remains callable in the worker');
Slim::Player::ProtocolHandlers->registerHandler('guest', 'Guest::Resolver');
is(Slim::Player::ProtocolHandlers->handlerForURL('guest://track:42'), 'Guest::Resolver', 'media scheme selects the registered resolver');
Slim::Player::ProtocolHandlers->registerURLHandler(qr{^https://example\.org/track/}, 'Guest::WebResolver');
is(Slim::Player::ProtocolHandlers->handlerForURL('https://example.org/track/42'), 'Guest::WebResolver', 'guest URL pattern selects its resolver without host service logic');
ok(!defined Slim::Player::ProtocolHandlers->handlerForURL('https://elsewhere.example/track/42'), 'unrelated URLs do not acquire a plugin resolver');
done_testing;
