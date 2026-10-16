use strict;
use warnings;
use Test::More;
use File::Temp qw(tempdir);
use File::Path qw(make_path);
use Chilpa::Web;
use Slim::Web::Settings;

{
    package Guest::Preferences;
    sub get { $_[0]{$_[1]} }
    sub set { $_[0]{$_[1]} = $_[2] }

    package Guest::Settings;
    our @ISA = ('Slim::Web::Settings');
    our $PREFS = bless {}, 'Guest::Preferences';
    sub page { 'plugins/guest/settings.html' }
    sub prefs { ($PREFS, 'username', 'identity_token') }
    sub handler {
        my ($class, $client, $params, $callback, $http, $response) = @_;
        if ($params->{connect}) {
            $response->code(302);
            $response->header(Location => 'auth.html');
        }
        return $class->SUPER::handler($client, $params);
    }

    package Guest::Auth;
    our @ISA = ('Slim::Web::Settings');
    our $COMPLETE;
    sub page { 'plugins/guest/auth.html' }
    sub handler {
        my ($class, $client, $params, $callback, @args) = @_;
        $COMPLETE = sub {
            $params->{challenge} = 'private challenge';
            $callback->($client, $params, $class->SUPER::handler($client, $params), @args);
        };
        return;
    }
}

my $root = tempdir(CLEANUP => 1);
make_path("$root/HTML/EN/plugins/guest");
for my $file ('settings', 'auth') {
    open my $fh, '>', "$root/HTML/EN/plugins/guest/$file.html" or die $!;
    print $fh $file eq 'settings' ? <<'SETTINGS' : <<'AUTH';
[% PROCESS settings/header.html %]
<input name="pref_username" value="[% prefs.pref_username | html %]">
<input name="pref_identity_token" value="[% prefs.identity_token | html %]">
<input type="submit" name="connect" value="Connect">
[% PROCESS settings/footer.html %]
SETTINGS
[% PROCESS settings/header.html %]
<input type="hidden" name="challenge" value="[% challenge %]">
<a href="https://example.org/authorize">Authorize</a>
[% PROCESS settings/footer.html %]
AUTH
    close $fh;
}
my $web = Chilpa::Web->new(root => $root, shared => 'compat/perl/templates', strings => sub { $_[0] });
local $Chilpa::Web::ACTIVE = $web;
Guest::Settings->new;
Guest::Auth->new;
my $result;
$web->open(Guest::Settings->page, {}, sub { $result = shift });
is($result->{revision}, 1, 'empty profile opens a versioned form');
is($result->{form}{fields}[1]{control}{type}, 'secret', 'text token is classified as secret');
$web->submit(1, 'saveSettings', { pref_username => 'listener', pref_identity_token => 'private value' }, sub { $result = shift });
is($Guest::Settings::PREFS->{username}, 'listener', 'declared preferences are saved');
is($result->{values}{pref_username}, 'listener', 'form-name preference aliases survive rendering');
is($Guest::Settings::PREFS->{identity_token}, 'private value', 'secret reaches the guest preference store');
ok(!exists $result->{values}{pref_identity_token}, 'saved secret is absent from public form');
eval { $web->submit(1, 'connect', {}, sub {}) };
like($@, qr/stale settings form/, 'an old form cannot replay an action');
my $finished = 0;
$web->submit(2, 'connect', {}, sub { $result = shift; $finished++ });
is($finished, 0, 'asynchronous authentication stays pending');
eval { $web->open(Guest::Settings->page, {}, sub {}) };
like($@, qr/already pending/, 'another request cannot overwrite pending authentication');
$Guest::Auth::COMPLETE->();
is($finished, 1, 'callback completes the redirected page');
is($result->{links}[0]{url}, 'https://example.org/authorize', 'authorization link reaches the host');
$Guest::Auth::COMPLETE->();
is($finished, 1, 'duplicate callbacks do not complete twice');
eval { $web->open('unregistered.html', {}, sub {}) };
like($@, qr/unknown settings page/, 'unregistered pages cannot be dispatched');
done_testing;
