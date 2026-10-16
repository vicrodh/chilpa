use strict;
use warnings;
use Test::More;
use lib 'compat/perl';
use Slim::Utils::Strings qw(string);

is(string('SETUP_USERNAME') . string('COLON'), 'Username:', 'shared account labels render as human text');
is(string('PASSWORD') . string('COLON'), 'Password:', 'punctuation is not an untranslated key');
local $Slim::Utils::Strings::STRINGS{SETUP_USERNAME} = { EN => 'Account name' };
is(string('SETUP_USERNAME'), 'Account name', 'guest translations retain precedence');
done_testing;
