package Slim::Utils::Strings;

use strict;
use warnings;
use Exporter 'import';

our @EXPORT_OK = qw(string cstring);
our $LANGUAGE = 'EN';
our %STRINGS;
our %COMMON = (
    COLON => ':', SETUP_USERNAME => 'Username', USERNAME => 'Username',
    SETUP_PASSWORD => 'Password', PASSWORD => 'Password',
    YES => 'Yes', NO => 'No', APPLY => 'Apply', SAVE => 'Save', CANCEL => 'Cancel',
);

sub load {
    my ($class, $file) = @_;
    die 'translation file is too large' if -s $file > 1024 * 1024;
    open my $fh, '<:encoding(UTF-8)', $file or die 'translation file is unavailable';
    my $key;
    while (my $line = <$fh>) {
        $line =~ s/[\r\n]+$//;
        if ($line =~ /^([A-Z0-9_]+)\s*$/) {
            $key = $1;
        }
        elsif (defined($key) && $line =~ /^\t([A-Z]{2}(?:[-_][A-Z]{2})?)\t(.*)$/) {
            $STRINGS{$key}{$1} = $2;
        }
    }
    close $fh;
}

sub string {
    my ($key, @args) = @_;
    return '' unless defined $key;
    my $entries = $STRINGS{$key} || {};
    my ($base) = split /[-_]/, uc $LANGUAGE;
    my $value = $entries->{uc $LANGUAGE} // $entries->{$base} // $entries->{EN} // $COMMON{$key};
    $value //= join ' ', map { ucfirst lc } split /_/, $key;
    return @args ? sprintf($value, @args) : $value;
}

sub cstring { shift; string(@_) }
sub getString { string(@_) }
sub stringExists { exists $STRINGS{$_[0]} || exists $COMMON{$_[0]} }

1;
