package Chilpa::Environment;

use strict;
use warnings;
use Encode ();
use Chilpa::Contributions;

our %LIMITATIONS;

sub provide {
    my ($package) = @_;
    (my $file = "$package.pm") =~ s{::}{/}g;
    $INC{$file} = __FILE__;
}

sub limitation { $LIMITATIONS{$_[0]} = 1; return }

BEGIN {
    # This is the implemented interface profile, not an installed server.
    $main::VERSION = '7.9.0';
    no strict 'refs';
    *main::WEBUI = sub () { 1 };
    *{"main::$_"} = sub () { 0 } for qw(INFOLOG DEBUGLOG SCANNER STATISTICS PERFMON LOCALFILE NOMYSB TRANSCODING);
    *main::ISWINDOWS = sub () { $^O eq 'MSWin32' ? 1 : 0 };
    *main::ISMAC = sub () { $^O eq 'darwin' ? 1 : 0 };
}

package Slim::Utils::Log;
BEGIN { Chilpa::Environment::provide(__PACKAGE__) }
use Exporter 'import';
our @EXPORT = qw(logger logError logWarning logBacktrace);
our %COUNTS;
sub logger { bless {}, __PACKAGE__ }
sub addLogCategory { logger() }
# Guest logs can contain tokens and signed URLs. I expose only severity counts.
sub error { $COUNTS{error}++ }
sub warn { $COUNTS{warn}++ }
sub info { $COUNTS{info}++ }
sub debug { $COUNTS{debug}++ }
sub logError { error() }
sub logWarning { $COUNTS{warn}++ }
sub logBacktrace { error() }
sub is_info { 0 } sub is_debug { 0 } sub is_warn { 1 } sub is_error { 1 }

package Slim::Utils::Versions;
BEGIN { Chilpa::Environment::provide(__PACKAGE__) }
sub compareVersions {
    my ($class, $left, $right) = @_;
    my @left = split /\./, $left;
    my @right = split /\./, $right;
    for my $i (0 .. (@left > @right ? $#left : $#right)) {
        my $difference = ($left[$i] || 0) <=> ($right[$i] || 0);
        return $difference if $difference;
    }
    return 0;
}

package Slim::Utils::Text;
BEGIN { Chilpa::Environment::provide(__PACKAGE__) }
sub ignoreCase { lc($_[0] // '') }
sub matchCase { uc($_[0] // '') }
sub ignorePunct { my $text = $_[0] // ''; $text =~ s/[[:punct:]]+/ /g; $text =~ s/\s+/ /g; $text }
sub ignoreCaseArticles { my $text = lc($_[0] // ''); $text =~ s/^(?:the|a|an|el|la|los|las|le|les)\s+//; $text }

package Slim::Utils::DateTime;
BEGIN { Chilpa::Environment::provide(__PACKAGE__) }
use POSIX ();
sub shortDateF { POSIX::strftime($_[1] || '%Y-%m-%d', localtime($_[0] // time)) }
sub longDateF { shortDateF(@_) }
sub timeF { POSIX::strftime($_[1] || '%H:%M', localtime($_[0] // time)) }
sub timeFormat { '%H:%M:%S' }

package Slim::Utils::Misc;
BEGIN { Chilpa::Environment::provide(__PACKAGE__) }
sub isWebBrowser { 0 }
sub canFollowWeblinks { 0 }
sub unescape { my $text = shift; $text =~ s/%([a-fA-F0-9]{2})/chr(hex($1))/eg; $text }

package Slim::Utils::Unicode;
BEGIN { Chilpa::Environment::provide(__PACKAGE__) }
sub utf8toLatin1Transliterate { require Text::Unidecode; Text::Unidecode::unidecode($_[0] // '') }
sub utf8decode { Encode::decode('UTF-8', $_[0]) }
sub utf8encode { Encode::encode('UTF-8', $_[0]) }
sub utf8off { Encode::_utf8_off($_[0]); $_[0] }
sub utf8on { Encode::_utf8_on($_[0]); $_[0] }

package Slim::Control::Request;
BEGIN { Chilpa::Environment::provide(__PACKAGE__) }
sub addDispatch {
    my ($words, $definition) = @_;
    $Chilpa::Contributions::ACTIVE->register('command', join(' ', @$words), $definition);
}
sub notifyFromArray { Chilpa::Environment::limitation('control notifications') }

package Slim::Player::Protocols::HTTP;
BEGIN { Chilpa::Environment::provide(__PACKAGE__) }
sub isRemote { 1 }
sub shouldLoop { 0 }

package Slim::Player::Protocols::HTTPS;
BEGIN { Chilpa::Environment::provide(__PACKAGE__) }
our @ISA = ('Slim::Player::Protocols::HTTP');

package Slim::Formats::Playlists;
BEGIN { Chilpa::Environment::provide(__PACKAGE__) }
sub registerParser { Chilpa::Environment::limitation('guest playlist parsers') }

package Slim::Web::ImageProxy;
BEGIN { Chilpa::Environment::provide(__PACKAGE__) }
sub proxiedImage { $_[0] }
sub registerHandler {
    my ($class, %definition) = @_;
    my $entries = $Chilpa::Contributions::ACTIVE->entries('image');
    $Chilpa::Contributions::ACTIVE->register('image', 'provider-' . scalar(keys %$entries), \%definition);
    Chilpa::Environment::limitation('guest image transformations');
}

package Slim::Music::Import;
BEGIN { Chilpa::Environment::provide(__PACKAGE__) }
sub addImporter { Chilpa::Environment::limitation('guest library importer') }

package Slim::Utils::Scanner::Remote;
BEGIN { Chilpa::Environment::provide(__PACKAGE__) }
sub parseRemoteHeader {
    my ($track, $url, $format, $success, $failure) = @_;
    Chilpa::Environment::limitation('guest remote header scanning');
    $failure->(undef, 'Header scanning belongs to the host audio pipeline.');
}

1;
